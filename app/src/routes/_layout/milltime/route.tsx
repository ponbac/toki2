import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_layout/milltime")({
        loader: ({ context }) =>
                context.queryClient.ensureQueryData(queries.differs()),
        component: MilltimeComponent,
});

function MilltimeComponent() {
        return (
                <div>
                        <h1>Milltime</h1>
                </div>
        );
}
