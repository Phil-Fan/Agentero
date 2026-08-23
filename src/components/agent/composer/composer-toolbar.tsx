import { CheckIcon, ChevronDown, Zap } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ComposerAttachImageButton } from "@/components/agent/composer/composer-attachments";
import {
	Context,
	ContextContent,
	ContextContentHeader,
	ContextTrigger,
} from "@/components/ai-elements/context";
import { PromptInputButton } from "@/components/ai-elements/prompt-input";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { InputGroupButton } from "@/components/ui/input-group";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import type { AgentEffortChoice, AgentModeChoice } from "@/lib/agent";
import { cn } from "@/lib/core/utils";

/** Renders inside `PromptInputTools`; the attach-image button needs the PromptInput attachment context. */
export function ComposerToolbar({
	compact = false,
	switching,
	collaborationOptions,
	collaborationModeId,
	selectedCollaborationName,
	onPickCollaborationMode,
	effortOptionsInDisplayOrder,
	reasoningEffort,
	onReasoningEffortChange,
	formatEffort,
	activeUsage,
	fastAvailable,
	fastEnabled,
	onFastEnabledToggle,
}: {
	compact?: boolean;
	switching: boolean;
	collaborationOptions: AgentModeChoice[];
	collaborationModeId: string | null;
	selectedCollaborationName: string | null;
	onPickCollaborationMode: (id: string) => void;
	effortOptionsInDisplayOrder: AgentEffortChoice[];
	reasoningEffort: string | null;
	onReasoningEffortChange: (id: string) => void;
	formatEffort: (value: string) => string;
	activeUsage: { used: number; size: number } | null;
	fastAvailable: boolean;
	fastEnabled: boolean;
	onFastEnabledToggle: () => void;
}) {
	const { t } = useTranslation("agent");
	return (
		<>
			{!compact && collaborationOptions.length > 0 ? (
				<DropdownMenu>
					<Tooltip>
						<TooltipTrigger asChild>
							<DropdownMenuTrigger asChild>
								<InputGroupButton
									type="button"
									size="sm"
									className="h-7 max-w-[min(10rem,100%)] gap-1 px-1.5 text-xs font-medium text-foreground"
								>
									<span className="truncate">
										{t("composer.collaboration.label")}:{" "}
										{selectedCollaborationName ??
											collaborationModeId ??
											t("composer.collaboration.label")}
									</span>
									<ChevronDown className="size-3 shrink-0 opacity-70" />
								</InputGroupButton>
							</DropdownMenuTrigger>
						</TooltipTrigger>
						<TooltipContent side="top">
							{t("composer.collaborationTooltip")}
						</TooltipContent>
					</Tooltip>
					<DropdownMenuContent align="start" className="min-w-36 p-1">
						{collaborationOptions.map((mode) => (
							<DropdownMenuItem
								key={mode.id}
								className={cn(
									"flex items-center justify-between gap-2 rounded-md",
									collaborationModeId === mode.id && "bg-muted",
								)}
								onSelect={() => onPickCollaborationMode(mode.id)}
							>
								<span className="truncate">{mode.name}</span>
								{collaborationModeId === mode.id ? (
									<CheckIcon className="size-3.5 shrink-0 text-muted-foreground" />
								) : null}
							</DropdownMenuItem>
						))}
					</DropdownMenuContent>
				</DropdownMenu>
			) : null}
			{!compact && effortOptionsInDisplayOrder.length > 0 ? (
				<DropdownMenu>
					<Tooltip>
						<TooltipTrigger asChild>
							<DropdownMenuTrigger asChild>
								<InputGroupButton
									type="button"
									size="sm"
									className="h-7 max-w-[min(8rem,100%)] gap-1 px-1.5 text-xs font-medium text-foreground"
								>
									<span className="truncate">
										{t("composer.effort.label")}:{" "}
										{formatEffort(reasoningEffort ?? "medium")}
									</span>
									<ChevronDown className="size-3 shrink-0 opacity-70" />
								</InputGroupButton>
							</DropdownMenuTrigger>
						</TooltipTrigger>
						<TooltipContent side="top">
							{t("composer.effortTooltip")}
						</TooltipContent>
					</Tooltip>
					<DropdownMenuContent align="start" className="min-w-28 p-1">
						{effortOptionsInDisplayOrder.map((effort) => (
							<DropdownMenuItem
								key={effort.id}
								className={cn(
									"justify-between rounded-md",
									reasoningEffort === effort.id && "bg-muted",
								)}
								onSelect={() => onReasoningEffortChange(effort.id)}
							>
								{formatEffort(effort.id)}
								{reasoningEffort === effort.id ? (
									<CheckIcon className="size-3.5 text-muted-foreground" />
								) : null}
							</DropdownMenuItem>
						))}
					</DropdownMenuContent>
				</DropdownMenu>
			) : null}
			{!compact && activeUsage && activeUsage.size > 0 ? (
				<Context usedTokens={activeUsage.used} maxTokens={activeUsage.size}>
					<ContextTrigger className="h-7 gap-1 px-1.5 text-xs" />
					<ContextContent>
						<ContextContentHeader />
					</ContextContent>
				</Context>
			) : null}
			{!compact && fastAvailable ? (
				<PromptInputButton
					type="button"
					className={cn(
						"size-7 text-foreground",
						fastEnabled && "text-amber-500 hover:text-amber-500",
					)}
					aria-pressed={fastEnabled}
					onClick={onFastEnabledToggle}
					tooltip={t("composer.fastToggle")}
				>
					<Zap
						className={cn(
							"size-3.5",
							fastEnabled &&
								"fill-amber-400 text-amber-500 dark:fill-amber-300 dark:text-amber-300",
						)}
					/>
				</PromptInputButton>
			) : null}
			{compact ? null : <ComposerAttachImageButton disabled={switching} />}
		</>
	);
}
