import Programmer from "@/components/programmer";
import DefaultLayout from "@/layouts/default";
import { Card, CardBody } from "@nextui-org/react";

export default function ProgrammerPage() {
    return <DefaultLayout>
        <Card radius="none">
            <CardBody>
                <Programmer/>
            </CardBody>
        </Card>
    </DefaultLayout>
}