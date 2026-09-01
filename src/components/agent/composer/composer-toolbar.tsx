import { Zap } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ComposerAttachImageButton } from "@/components/agent/composer/composer-attachments";
import {
	Context,
	ContextContent,
	ContextContentHeader,
	ContextTrigger,
} from "@/components/ai-elements/context";
import { PromptInputButton } from "@/components/ai-elements/prompt-input";
import { cn } from "@/lib/core/utils";

/** Renders inside `PromptInputTools`; the attach-image button needs the PromptInput attachment context. */
export function ComposerToolbar({
	compact = false,
	switching,
	activeUsage,
	fastAvailable,
	fastEnabled,
	onFastEnabledToggle,
}: {
	compact?: boolean;
	switching: boolean;
	activeUsage: { used: number; size: number } | null;
	fastAvailable: boolean;
	fastEnabled: boolean;
	onFastEnabledToggle: () => void;
}) {
	const { t } = useTranslation("agent");
	return (
		<>
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
