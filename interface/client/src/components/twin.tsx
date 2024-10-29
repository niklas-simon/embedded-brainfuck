import { Table, TableBody, TableCell, TableColumn, TableHeader, TableRow } from "@nextui-org/table";
import Tape from "./tape";
import { Card, CardBody } from "@nextui-org/card";
import { Chip } from "@nextui-org/chip";
import { useEffect, useState } from "react";
import RestService, { State } from "@/run-service";
import { Skeleton } from "@nextui-org/skeleton";

const states = [
    {
        text: "Running",
        color: "success",
        on: state => state.control === "running" && !state.jumping
    },
    {
        text: "Starting",
        color: "warning",
        on: state => state.control === "startup"
    },
    {
        text: "Jumping",
        color: "warning",
        on: state => state.jumping
    },
    {
        text: "Waiting for input",
        color: "danger",
        on: state => state.control === "wait_input"
    },
    {
        text: "Waiting for output",
        color: "danger",
        on: state => state.control === "output_ready"
    },
    {
        text: "Idle",
        color: "success",
        on: state => state.control === "idle"
    }
 ] as {text: string, color: "success" | "danger" | "warning", on: (state: State) => boolean}[]

const rs = RestService.getInstance();

export default function Twin() {
    const [program, setProgram] = useState<string | null>(rs.getProgram());
    const [input, setInput] = useState<string | null>(rs.getInput());
    const [state, setState] = useState<State | null>(rs.getState());

    useEffect(() => {
        const programProfile = rs.onProgramChange(setProgram);
        const inputProfile = rs.onInputChange(setInput);
        const stateProfile = rs.onStateChange(setState);
        
        return () => {
            rs.removeListener(programProfile);
            rs.removeListener(inputProfile);
            rs.removeListener(stateProfile);
        }
    }, []);

    const inputQueue = input?.split("").slice(state?.ic) || [];

    return <div className="w-full h-full flex flex-col items-center justify-center overflow-hidden">
        {"uncontrolled" === state?.control ?
            <Chip color="warning" variant="flat">currently not controlled or idle</Chip>
        :
            <Skeleton isLoaded={state !== null}>
                <Card className="bg-content2 overflow-visible" radius="none">
                    <CardBody className="flex flex-col items-center overflow-visible gap-4">
                        <Tape title="Program" data={Array.from(new Array(7)).map((_, i) => {
                            if (!state || !program) {
                                return null;
                            }
                            const pc = state.code?.pc || 0;
                            if (pc - 3 + i < 0) {
                                return null;
                            }
                            if (pc - 3 + i >= program.length) {
                                return null;
                            }
                            return {
                                address: pc - 3 + i,
                                value: program[pc - 3 + i]
                            }
                        })}/>
                        <Tape title="Memory" data={state?.tape?.map((v, i) => {
                            const head = state.head || 0;
                            return {
                                address: head - 3 + i,
                                value: v
                            }
                        }) || []}/>
                        <div className="flex flex-row gap-2 justify-start w-full">
                            <Table radius="none" isCompact={true} fullWidth={false} removeWrapper={true} className="bg-content1 w-min" aria-label="Stack"> 
                                <TableHeader>
                                    <TableColumn className="bg-content1 rounded-none h-min pt-2">Stack</TableColumn>
                                </TableHeader>
                                <TableBody>
                                    {state?.stack?.length ? 
                                        state.stack.slice(0, 6).map((v, i) => {
                                            return <TableRow key={i}>
                                                <TableCell className="font-mono">
                                                    {state.stack!.length > 6 && i === 5 ? "..." : "0x" + v.toString(16).padStart(4, "0")}
                                                </TableCell>
                                            </TableRow>
                                        })
                                    :
                                        <TableRow>
                                            <TableCell>
                                                (empty)
                                            </TableCell>
                                        </TableRow>
                                    }
                                </TableBody>
                            </Table>
                            <div className="flex flex-col gap-2">
                                {states.map((s, i) => {
                                    return <Chip key={i} variant="dot" className="border-none" color={state && s.on(state) ? s.color : "default"}>{s.text}</Chip>
                                })}
                            </div>
                        </div>
                        <div className="flex flex-col w-[232px] overflow-hidden gap-1" style={{
                            maskImage: "linear-gradient(to left, transparent, black 64px)"
                        }}>
                            <span className="text-foreground-500 text-tiny">Input queue</span>
                            <div className="flex flex-row gap-2 font-mono w-max">
                                {inputQueue.length > 0 ? inputQueue.map((v, i) => <div key={i} className="p-1 bg-content1 w-[1.5em] text-center">{v}</div>) : <span>(empty)</span>}
                            </div>
                        </div>
                    </CardBody>
                </Card>
            </Skeleton>
        }
    </div>
}