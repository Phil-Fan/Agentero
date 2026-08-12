import { Bot, Library } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/core/utils";

export type MobileTab = "library" | "agent";

const TABS: Array<{ id: MobileTab; icon: typeof Library }> = [
	{ id: "library", icon: Library },
	{ id: "agent", icon: Bot },
];

/** Matches the nav's `gap-1`, so the highlight can step item height + gap. */
const TAB_GAP = "0.25rem";

export function MobileNav({
	tab,
	onTab,
	variant = "rail",
}: {
	tab: MobileTab;
	onTab: (tab: MobileTab) => void;
	variant?: "rail" | "sidebar";
}) {
	const { t } = useTranslation("mobile");
	const sidebar = variant === "sidebar";
	const activeIndex = Math.max(
		0,
		TABS.findIndex((entry) => entry.id === tab),
	);

	return (
		<nav
			aria-label={t("tabs.navigation")}
			className={cn(
				"relative flex gap-1",
				sidebar ? "flex-col" : "mt-10 flex-col",
			)}
		>
			{/* One highlight for the whole nav: it slides between tabs instead of
			    each row snapping its own background on and off. */}
			<span
				aria-hidden
				className="absolute inset-x-0 top-0 rounded-lg bg-muted/80 transition-transform duration-200 ease-out"
				style={{
					height: `calc((100% - ${TABS.length - 1} * ${TAB_GAP}) / ${TABS.length})`,
					transform: `translateY(calc(${activeIndex} * (100% + ${TAB_GAP})))`,
				}}
			/>
			{TABS.map(({ id, icon: Icon }) => (
				<button
					key={id}
					type="button"
					onClick={() => onTab(id)}
					className={cn(
						"relative flex items-center rounded-lg outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
						sidebar
							? "h-12 w-full gap-3 px-3 text-base"
							: "size-10 justify-center px-0",
						tab === id
							? "font-medium text-foreground"
							: "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
					)}
					aria-current={tab === id ? "page" : undefined}
				>
					<Icon
						className="size-5 shrink-0"
						strokeWidth={tab === id ? 2.25 : 2}
					/>
					{sidebar ? <span>{t(`tabs.${id}`)}</span> : null}
				</button>
			))}
		</nav>
	);
}
