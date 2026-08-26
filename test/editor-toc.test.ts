import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { HeadingElement } from "@/components/editor/nodes/block/heading-node";

describe("Markdown table of contents", () => {
	it("mirrors each Plate heading node id to its DOM id for scroll tracking", () => {
		const headingId = "heading-node-id";
		const markup = renderToStaticMarkup(
			createElement(HeadingElement, {
				attributes: {},
				children: "Heading",
				editor: {
					api: {
						isBlock: () => true,
					},
				},
				element: {
					id: headingId,
					type: "h2",
					children: [{ text: "Heading" }],
				},
				variant: "h2",
			} as unknown as Parameters<typeof HeadingElement>[0]),
		);

		expect(markup).toContain(`id="${headingId}"`);
	});

	it("collapses the first heading's top margin and keeps a modest h1 gap", () => {
		const markup = renderToStaticMarkup(
			createElement(HeadingElement, {
				attributes: {},
				children: "Title",
				editor: {
					api: {
						isBlock: () => true,
					},
				},
				element: {
					id: "h1-spacing",
					type: "h1",
					children: [{ text: "Title" }],
				},
				variant: "h1",
			} as unknown as Parameters<typeof HeadingElement>[0]),
		);

		expect(markup).toContain("first:mt-0");
		expect(markup).toContain("mt-[1em]");
		expect(markup).not.toContain("mt-[1.6em]");
	});
});
