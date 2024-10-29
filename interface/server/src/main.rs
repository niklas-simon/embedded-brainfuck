use std::{
    io::Cursor,
    sync::{
        mpsc::{self, Sender},
        OnceLock, RwLock,
    },
    time::Instant,
};

use bf_itp::Run;
use rocket::{
    fs::{relative, FileServer},
    http::{ContentType, Status},
    response::{self, Responder},
    Request, Response,
};
use serde_json::{json, Value};

mod api;
// attempt to check whether this is a raspberrry pi or not.
// if it is, use the 'real' module for hardware interaction.
// otherwise, use a mock.
#[cfg(all(target_arch = "aarch64", target_env = "gnu"))]
mod hw;
#[cfg(not(all(target_arch = "aarch64", target_env = "gnu")))]
#[path = "hw_mock.rs"]
mod hw;

// have a cat

pub type BFRes = Result<(), BFError>;

#[rocket::main]
async fn main() {
    let (sx, tx) = mpsc::channel::<HWCmd>();
    GLOBAL_STATE
        .set(Global::new(sx))
        .expect("global already initialised");
    let _hw_runner = hw::start_hw_thread(tx);
    start_rocket().await;
}

async fn start_rocket() {
    rocket::build()
        .mount("/api", api::get_routes())
        .mount("/", FileServer::from(relative!("static")))
        .launch()
        .await
        .expect("failed to launch rocket");
}

#[derive(Debug)]
pub enum ItpState {
    Idle,
    Startup,
    Running { run: Run, paused: bool },
    // usize: input counter
    Uncontrolled(usize),
}

static GLOBAL_STATE: OnceLock<Global> = OnceLock::new();

#[derive(Debug)]
pub struct ChangeTimes {
    speed: RwLock<Instant>,
    code: RwLock<Instant>,
    input: RwLock<Instant>,
    output: RwLock<Instant>,
    state: RwLock<Instant>,
}

impl ChangeTimes {
    pub fn new() -> Self {
        Self {
            speed: RwLock::new(Instant::now()),
            code: RwLock::new(Instant::now()),
            input: RwLock::new(Instant::now()),
            output: RwLock::new(Instant::now()),
            state: RwLock::new(Instant::now()),
        }
    }
}

#[derive(Debug)]
pub struct Global {
    hw: Sender<HWCmd>,
    // speed: 1..=100
    // frequency = 10^(3 * log10(speed)) = speed^3
    // interval = 1 / frequency
    speed: RwLock<u8>,
    full_code: RwLock<String>,
    input: RwLock<String>,
    output: RwLock<String>,
    state: RwLock<ItpState>,
    hw_state: RwLock<HWState>,
    last_change: ChangeTimes,
}

impl Global {
    pub fn new(hw: Sender<HWCmd>) -> Self {
        Self {
            hw,
            speed: RwLock::new(100),
            full_code: RwLock::new(String::new()),
            input: RwLock::new(String::new()),
            output: RwLock::new(String::new()),
            state: RwLock::new(ItpState::Idle),
            hw_state: RwLock::new(HWState::Regular),
            last_change: ChangeTimes::new(),
        }
    }
}

impl Global {
    fn set_input(&self, inp: String) {
        *self.input.write().unwrap() = inp;
        *self.last_change.input.write().unwrap() = Instant::now();
    }

    fn set_code(&self, code: String) {
        *self.full_code.write().unwrap() = code;
        *self.last_change.code.write().unwrap() = Instant::now();
    }

    fn set_output(&self, output: String) {
        *self.output.write().unwrap() = output;
        *self.last_change.output.write().unwrap() = Instant::now();
    }

    fn set_state(&self, state: ItpState) {
        *self.state.write().unwrap() = state;
        *self.last_change.state.write().unwrap() = Instant::now();
    }

    pub fn set_speed(&self, speed: u8) {
        *self.speed.write().unwrap() = speed;
        *self.last_change.speed.write().unwrap() = Instant::now();
    }

    pub fn change_input(&self, inp: String) -> BFRes {
        match *self.state.write().unwrap() {
            ItpState::Running { ref mut run, .. } => {
                let curr = self.input.read().unwrap();
                if inp.len() >= run.ic && curr[..run.ic] == inp[..run.ic] {
                    run.inp = inp.as_bytes().to_vec();
                    self.set_input(inp);
                    Ok(())
                } else {
                    Err(BFError::InputChanged)
                }
            }
            _ => {
                self.set_input(inp);
                Ok(())
            }
        }
    }

    pub fn change_code(&self, code: String) -> BFRes {
        if !bf_itp::is_nesting_correct(&code) {
            return Err(BFError::InvalidNesting);
        }
        match *self.state.read().unwrap() {
            ItpState::Idle => {
                self.set_code(code);
                Ok(())
            }
            ItpState::Startup => Err(BFError::CodeChanged),
            ItpState::Running { .. } => Err(BFError::CodeChanged),
            ItpState::Uncontrolled(_) => {
                self.set_code(code);
                self.send_hw(HWCmd::Program);
                Ok(())
            }
        }
    }

    pub fn get_state(&self) -> Value {
        let ctrl = match *self.hw_state.read().unwrap() {
            HWState::Regular => match *self.state.read().unwrap() {
                ItpState::Idle => "idle",
                ItpState::Startup => "startup",
                ItpState::Running { paused, .. } => {
                    if paused {
                        "paused"
                    } else {
                        "running"
                    }
                }
                ItpState::Uncontrolled(_) => "uncontrolled",
            },
            HWState::WaitInput => "wait_input",
            HWState::OutputReady => "output_ready",
        };
        match *self.state.read().unwrap() {
            ItpState::Running { ref run, .. } => {
                serde_json::to_value(run.view(ctrl)).unwrap_or(json!({"control": ctrl}))
            }
            _ => json!({"control": ctrl}),
        }
    }

    /// queue a hw task
    pub fn send_hw(&self, cmd: HWCmd) {
        self.hw.send(cmd).expect("hardware thread died");
    }

    /// start digital twin
    /// 
    /// clear output and set state to [`ItpState::Running`]
    /// 
    /// to be called by the hw_runner thread when the hw interpreter was started successfully
    pub fn itp_started(&self, paused: bool) {
        self.set_output(String::new());
        *self.hw_state.write().unwrap() = HWState::Regular;
        let code = &*self.full_code.read().unwrap();
        let input = &*self.input.read().unwrap();
        let run = Run::new(code, input).expect("code should have already been checked");
        self.set_state(ItpState::Running { run, paused });
        println!("run started");
    }
}

/// responses to invalid requests
pub enum BFError {
    /// trying to change code while itp is running
    CodeChanged,
    /// change input while itp running
    InputChanged,
    /// code is not correctly nested
    InvalidNesting,
    /// tried to apply control when control is disabled
    ItpUncontrolled,
    /// interpreter already running
    ItpRunning,
    /// interpreter not running
    ItpNotRunning,
    /// waiting for startup to finish
    StillStarting,
}

impl<'r> Responder<'r, 'static> for BFError {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        let (status, text) = match self {
            BFError::CodeChanged => (
                Status::UnprocessableEntity,
                "cannot change code while interpreter is running",
            ),
            BFError::InputChanged => (
                Status::UnprocessableEntity,
                "cannot change already read input during run",
            ),
            BFError::InvalidNesting => (Status::UnprocessableEntity, "code is not correctly nested"),
            BFError::ItpUncontrolled => (Status::BadRequest, "control is currently not enabled"),
            BFError::ItpRunning => (Status::BadRequest, "interpreter is currently running"),
            BFError::ItpNotRunning => (Status::BadRequest, "interpreter is currently not running"),
            BFError::StillStarting => (Status::BadRequest, "interpreter is still starting"),
        };
        Response::build()
            .header(ContentType::Plain)
            .sized_body(text.len(), Cursor::new(text))
            .status(status)
            .ok()
    }
}

/// tasks for hw interaction
pub enum HWCmd {
    /// disable control
    EndControl,
    /// enable control
    StartControl,
    /// write code to hw
    Program,
    /// start a new run
    /// 
    /// can start paused (with /api/ctrl/step) or running (with /api/ctrl/start)
    StartRun(bool),
    /// execute a single step
    ExecStep,
    /// reset interpreter
    Reset,
}

/// special hardware states
/// 
/// set by the hw_runner thread to display when the
/// hw interpreter is waiting for input or has some
/// output ready
#[derive(Debug)]
pub enum HWState {
    Regular,
    WaitInput,
    OutputReady,
}
