import type { ReactNode } from "react";
import { Label } from "@/components/ui/label";

export function PageTitle({
	title,
	actions,
}: {
	title: ReactNode;
	actions?: ReactNode;
}) {
	if (!actions) {
		return (
			<h2 className="mb-4 font-semibold text-lg tracking-tight">{title}</h2>
		);
	}
	return (
		<div className="mb-4 flex items-center justify-between gap-3">
			<h2 className="font-semibold text-lg tracking-tight">{title}</h2>
			{actions}
		</div>
	);
}

export function SettingsGroup({ children }: { children: ReactNode }) {
	return (
		<div className="mb-5">
			<div className="overflow-hidden rounded-xl border bg-card">
				{children}
			</div>
		</div>
	);
}

export function SettingsRow({
	label,
	htmlFor,
	description,
	children,
}: {
	label: ReactNode;
	htmlFor?: string;
	/** Optional muted secondary line under the label. */
	description?: string;
	children: ReactNode;
}) {
	return (
		<div className="flex items-center justify-between gap-4 border-b px-3.5 py-2.5 last:border-b-0">
			<div className="min-w-0">
				<Label htmlFor={htmlFor} className="font-normal text-[13px]">
					{label}
				</Label>
				{description ? (
					<p className="mt-0.5 text-muted-foreground text-xs leading-relaxed">
						{description}
					</p>
				) : null}
			</div>
			<div className="shrink-0">{children}</div>
		</div>
	);
}
