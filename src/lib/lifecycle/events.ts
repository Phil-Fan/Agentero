export type LifecycleEventMap = {
	"app:ready": { timestamp: number };
	"vault:opened": { vaultId: string; timestamp: number };
	"paper:imported": { vaultId: string; paperId: string; timestamp: number };
	"paper:assets-ready": { vaultId: string; paperId: string; timestamp: number };
	"paper:opened": { paperId: string; timestamp: number };
	"job:completed": {
		jobId: string;
		kind: string;
		paperId?: string;
		timestamp: number;
	};
	"job:failed": {
		jobId: string;
		kind: string;
		paperId?: string;
		error?: string;
		timestamp: number;
	};
};

export type LifecycleEvent = keyof LifecycleEventMap;
