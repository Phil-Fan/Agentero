import { describe, expect, it } from "vitest";
import { agentLogoKeyForTemplate } from "@/components/agent/agent-logo";

describe("agentLogoKeyForTemplate", () => {
	it("maps built-in ACP templates to stable logo keys", () => {
		expect(agentLogoKeyForTemplate("opencode")).toBe("opencode");
		expect(agentLogoKeyForTemplate("openclaw")).toBe("openclaw");
		expect(agentLogoKeyForTemplate("claude-acp")).toBe("claude-acp");
		expect(agentLogoKeyForTemplate("codex-acp")).toBe("codex-acp");
		expect(agentLogoKeyForTemplate("gemini")).toBe("gemini");
		expect(agentLogoKeyForTemplate("hermes")).toBe("hermes");
		expect(agentLogoKeyForTemplate("qodercli")).toBe("qodercli");
		expect(agentLogoKeyForTemplate("grok-build")).toBe("grok-build");
		expect(agentLogoKeyForTemplate("pi")).toBe("pi");
	});

	it("falls back to the custom logo for unknown or empty templates", () => {
		expect(agentLogoKeyForTemplate("unknown")).toBe("custom");
		expect(agentLogoKeyForTemplate(null)).toBe("custom");
		expect(agentLogoKeyForTemplate(undefined)).toBe("custom");
	});
});
