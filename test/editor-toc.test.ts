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
		const first = renderToStaticMarkup(
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
				path: [0],
				variant: "h1",
			} as unknown as Parameters<typeof HeadingElement>[0]),
		);
		const later = renderToStaticMarkup(
			createElement(HeadingElement, {
				attributes: {},
				children: "Later",
				editor: {
					api: {
						isBlock: () => true,
					},
				},
				element: {
					id: "h1-later",
					type: "h1",
					children: [{ text: "Later" }],
				},
				path: [1],
				variant: "h1",
			} as unknown as Parameters<typeof HeadingElement>[0]),
		);

		expect(first).toContain("mt-0");
		expect(later).toContain("mt-6");
		expect(later).not.toContain("mt-0");
		expect(later).not.toContain("mt-[1em]");
	});
});
