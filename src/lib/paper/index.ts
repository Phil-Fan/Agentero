export {
	paperAssetDownloadReasons,
	paperHasLocalPaperMd,
	paperHasLocalPdf,
	paperHasLocalTex,
	paperNeedsAssetDownload,
	paperNeedsRead,
} from "@/lib/paper/assets";
export {
	collectPaperFoldersFromTree,
	detectPaperDirectory,
	directoryHasPaperMarkers,
	isPaperDirectory,
	paperDirFromPath,
	resolvePapersParentDir,
} from "@/lib/paper/detect";
export {
	loadPaperMetadata,
	loadPaperOpenBundle,
	type PaperOpenBundle,
	paperCatalogPath,
} from "@/lib/paper/load-meta";
export {
	canAttemptPdfDownload,
	findLocalPdfPath,
	isPdfViewerSource,
	localFileToArrayBuffer,
	localImageToViewerSource,
	paperRemoteAssetsFromMetadata,
	revokePdfViewerSource,
} from "@/lib/paper/media";
export {
	isPapersRoot,
	isUnderPapers,
	notesPathForPaper,
} from "@/lib/paper/paths";
export {
	formatAuthorsShort,
	formatPaperTreeLabel,
	PAPER_TREE_LABEL_MODES,
	PAPER_TREE_SORT_MODES,
	type PaperTreeLabelMode,
	type PaperTreeSortMode,
	sortFileTreeNodes,
} from "@/lib/paper/tree-label";
export type { PaperMetadata, PaperTag } from "@/lib/paper/types";
