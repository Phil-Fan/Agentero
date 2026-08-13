import { Terminal } from "lucide-react";
import type { ComponentProps } from "react";
import { SiClaude, SiGooglegemini, SiHermes, SiOpencode } from "react-icons/si";
import type { AgentTemplate } from "@/lib/agent";
import { cn } from "@/lib/core/utils";

export type AgentLogoKey =
	| "opencode"
	| "openclaw"
	| "claude-acp"
	| "codex-acp"
	| "gemini"
	| "hermes"
	| "qodercli"
	| "grok-build"
	| "pi"
	| "custom";

export function agentLogoKeyForTemplate(
	template: AgentTemplate | string | null | undefined,
): AgentLogoKey {
	switch (template) {
		case "opencode":
		case "openclaw":
		case "claude-acp":
		case "codex-acp":
		case "gemini":
		case "hermes":
		case "qodercli":
		case "grok-build":
		case "pi":
			return template;
		default:
			return "custom";
	}
}

export function AgentLogo({
	template,
	className,
	iconClassName,
}: {
	template: AgentTemplate | string | null | undefined;
	className?: string;
	/** Override the inner icon size (default `size-3.5`). */
	iconClassName?: string;
}) {
	const key = agentLogoKeyForTemplate(template);
	const shellClass = cn(
		"inline-flex size-5 shrink-0 items-center justify-center rounded-md border border-border/70 bg-background text-foreground shadow-xs",
		className,
	);
	const iconClass = cn("size-3.5", iconClassName);

	switch (key) {
		case "opencode":
			return (
				<span className={shellClass} aria-hidden>
					<SiOpencode className={iconClass} />
				</span>
			);
		case "openclaw":
			return (
				<span
					className={cn(shellClass, "text-red-600 dark:text-red-400")}
					aria-hidden
				>
					<OpenClawMark className={iconClass} />
				</span>
			);
		case "claude-acp":
			return (
				<span className={cn(shellClass, "text-[#D97757]")} aria-hidden>
					<SiClaude className={iconClass} />
				</span>
			);
		case "codex-acp":
			return (
				<span className={shellClass} aria-hidden>
					<OpenAiMark className={iconClass} />
				</span>
			);
		case "gemini":
			return (
				<span className={cn(shellClass, "text-[#1A73E8]")} aria-hidden>
					<SiGooglegemini className={iconClass} />
				</span>
			);
		case "hermes":
			return (
				<span
					className={cn(shellClass, "text-[#8B5E3C] dark:text-[#D3A47A]")}
					aria-hidden
				>
					<SiHermes className={iconClass} />
				</span>
			);
		case "grok-build":
			return (
				<span className={shellClass} aria-hidden>
					<GrokMark className={iconClass} />
				</span>
			);
		case "pi":
			return (
				<span
					className={cn(shellClass, "text-violet-700 dark:text-violet-300")}
					aria-hidden
				>
					<PiMark className={iconClass} />
				</span>
			);
		case "qodercli":
			return (
				<span
					className={cn(
						shellClass,
						"font-semibold text-[10px] text-sky-700 dark:text-sky-300",
					)}
					aria-hidden
				>
					Q
				</span>
			);
		case "custom":
			return (
				<span className={cn(shellClass, "text-muted-foreground")} aria-hidden>
					<Terminal className={iconClass} />
				</span>
			);
	}
}

function OpenClawMark(props: ComponentProps<"svg">) {
	return (
		<svg viewBox="0 0 120 120" fill="none" aria-hidden {...props}>
			<title>OpenClaw</title>
			<path
				d="M60 10C30 10 15 35 15 55c0 20 15 40 30 45v10h10v-10c0 0 5 2 10 0v10h10v-10c15-5 30-25 30-45 0-20-15-45-45-45Z"
				fill="currentColor"
			/>
			<path
				d="M20 45C5 40 0 50 5 60c5 10 15 5 20-5 3-7 0-10-5-10ZM100 45c15-5 20 5 15 15s-15 5-20-5c-3-7 0-10 5-10Z"
				fill="currentColor"
			/>
			<path
				d="M45 15Q35 5 30 8M75 15Q85 5 90 8"
				stroke="currentColor"
				strokeWidth="3"
				strokeLinecap="round"
			/>
			<circle cx="45" cy="35" r="4.5" fill="var(--background)" />
			<circle cx="75" cy="35" r="4.5" fill="var(--background)" />
		</svg>
	);
}

function OpenAiMark(props: ComponentProps<"svg">) {
	return (
		<svg
			viewBox="0 0 24 24"
			fill="currentColor"
			fillRule="evenodd"
			aria-hidden
			{...props}
		>
			<title>OpenAI</title>
			<path d="M21.55 10.004a5.416 5.416 0 00-.478-4.501c-1.217-2.09-3.662-3.166-6.05-2.66A5.59 5.59 0 0010.831 1C8.39.995 6.224 2.546 5.473 4.838A5.553 5.553 0 001.76 7.496a5.487 5.487 0 00.691 6.5 5.416 5.416 0 00.477 4.502c1.217 2.09 3.662 3.165 6.05 2.66A5.586 5.586 0 0013.168 23c2.443.006 4.61-1.546 5.361-3.84a5.553 5.553 0 003.715-2.66 5.488 5.488 0 00-.693-6.497v.001zm-8.381 11.558a4.199 4.199 0 01-2.675-.954c.034-.018.093-.05.132-.074l4.44-2.53a.71.71 0 00.364-.623v-6.176l1.877 1.069c.02.01.033.029.036.05v5.115c-.003 2.274-1.87 4.118-4.174 4.123zM4.192 17.78a4.059 4.059 0 01-.498-2.763c.032.02.09.055.131.078l4.44 2.53c.225.13.504.13.73 0l5.42-3.088v2.138a.068.068 0 01-.027.057L9.9 19.288c-1.999 1.136-4.552.46-5.707-1.51h-.001zM3.023 8.216A4.15 4.15 0 015.198 6.41l-.002.151v5.06a.711.711 0 00.364.624l5.42 3.087-1.876 1.07a.067.067 0 01-.063.005l-4.489-2.559c-1.995-1.14-2.679-3.658-1.53-5.63h.001zm15.417 3.54l-5.42-3.088L14.896 7.6a.067.067 0 01.063-.006l4.489 2.557c1.998 1.14 2.683 3.662 1.529 5.633a4.163 4.163 0 01-2.174 1.807V12.38a.71.71 0 00-.363-.623zm1.867-2.773a6.04 6.04 0 00-.132-.078l-4.44-2.53a.731.731 0 00-.729 0l-5.42 3.088V7.325a.068.068 0 01.027-.057L14.1 4.713c2-1.137 4.555-.46 5.707 1.513.487.833.664 1.809.499 2.757h.001zm-11.741 3.81l-1.877-1.068a.065.065 0 01-.036-.051V6.559c.001-2.277 1.873-4.122 4.181-4.12.976 0 1.92.338 2.671.954-.034.018-.092.05-.131.073l-4.44 2.53a.71.71 0 00-.365.623l-.003 6.173v.002zm1.02-2.168L12 9.25l2.414 1.375v2.75L12 14.75l-2.415-1.375v-2.75z" />
		</svg>
	);
}

function GrokMark(props: ComponentProps<"svg">) {
	return (
		<svg
			viewBox="0 0 24 24"
			fill="currentColor"
			fillRule="evenodd"
			aria-hidden
			{...props}
		>
			<title>Grok</title>
			<path d="M9.27 15.29l7.978-5.897c.391-.29.95-.177 1.137.272.98 2.369.542 5.215-1.41 7.169-1.951 1.954-4.667 2.382-7.149 1.406l-2.711 1.257c3.889 2.661 8.611 2.003 11.562-.953 2.341-2.344 3.066-5.539 2.388-8.42l.006.007c-.983-4.232.242-5.924 2.75-9.383.06-.082.12-.164.179-.248l-3.301 3.305v-.01L9.267 15.292M7.623 16.723c-2.792-2.67-2.31-6.801.071-9.184 1.761-1.763 4.647-2.483 7.166-1.425l2.705-1.25a7.808 7.808 0 00-1.829-1A8.975 8.975 0 005.984 5.83c-2.533 2.536-3.33 6.436-1.962 9.764 1.022 2.487-.653 4.246-2.34 6.022-.599.63-1.199 1.259-1.682 1.925l7.62-6.815" />
		</svg>
	);
}

function PiMark(props: ComponentProps<"svg">) {
	return (
		<svg
			viewBox="0 0 800 800"
			fill="currentColor"
			fillRule="evenodd"
			aria-hidden
			{...props}
		>
			<title>Pi</title>
			<path d="M165.29 165.29H517.36V400H400V517.36H282.65V634.72H165.29ZM282.65 282.65V400H400V282.65Z" />
			<path d="M517.36 400H634.72V634.72H517.36Z" />
		</svg>
	);
}
