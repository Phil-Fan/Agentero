# 云同步（S3）

多设备间同步整个 Vault 到 S3 兼容对象存储（R2 / MinIO / AWS S3 / OSS / B2）。设计草稿与分期：[../development/cloud-sync-s3.md](../development/cloud-sync-s3.md)。当前已落地 Phase 0–1（sidecar 化 + 手动同步 MVP）。

## 模块

`src-tauri/src/features/sync/`（desktop-only）：

| 文件 | 职责 |
|---|---|
| `config.rs` | 凭据存 XDG `agentero/sync.json`（按 Vault 路径分键，0600）；`secretKey` 出站掩码 / 回传掩码保留旧值（同 translate API key 先例） |
| `s3.rs` | 最小 S3 客户端：GET / 条件 PUT（`If-Match` / `If-None-Match`）/ ListObjectsV2，reqwest + 手写 SigV4（HMAC-SHA256 自实现，RFC 4231 向量测试） |
| `snapshot.rs` | Vault 扫描 → `Manifest`（relPath → sha256/size/mtime）；`size+mtime` 未变复用 base 哈希；忽略 `.agentero` `.git` `node_modules` `.DS_Store` `*.tmp` |
| `local.rs` | `.agentero/vault.json`（Vault UUID）、`.agentero/sync/{base,state}.json`（watcher 忽略 `.agentero/`，无事件回环） |
| `engine.rs` | 三方合并 + 应用 + 发布（见下） |
| `commands.rs` | `sync_get_status` / `sync_configure` / `sync_disconnect` / `sync_now`；广播 `sync:state` / `sync:progress` 事件 |

## Remote 布局与一次同步

```text
<prefix>/vault.json                  { vaultId, formatVersion, encryption }
<prefix>/HEAD                        { version, manifestKey, updatedAt } ← 唯一可变对象，CAS 推进
<prefix>/manifests/<v>-<nonce>.json.gz
<prefix>/blobs/<aa>/<sha256>         gzip(内容)，内容寻址天然去重
```

一次 `sync_now`：扫描 → GET HEAD/manifest → 与本地 base（上次同步清单）三方合并 → 应用远端改动（临时文件 + rename 原子落盘，blob 校验 sha256）→ 上传新 blob（`If-None-Match: *`，跨设备重复上传为廉价 no-op）→ 发布新 manifest → `If-Match` CAS 推进 HEAD。CAS 失败（他端并发推进）则以对方 manifest 为新 base 重跑，最多 5 次。

合并规则：单侧改动直接采纳；双侧同改 `*.md` 保留 mtime 较新者、较旧者存为 `<name> (conflict <时间>).md`；其余文件（sidecar/marks/二进制）按 mtime LWW；删除 vs 修改保留修改。

## 身份与 Catalog 联动

- `vault.json`（远端）与 `.agentero/vault.json`（本地）配对：从未同步过的 Vault 可加入既有 remote（采纳其 id）；有同步历史的 Vault 拒绝外来 remote。
- 论文权威字段已 sidecar 化：每次 `upsert_paper` 同步投影到 `papers/<id>/metadata.json`；`paper_rescan` 优先从 sidecar 恢复（sidecar 较新则回灌 DB）。因此同步只处理普通文件，`catalog.sqlite` 不出 Vault；拉取后引擎自动 `rebuild_from_disk` + `prune_missing`。

## 前端

设置窗口「同步」pane：`src/components/settings/panes/sync-pane.tsx`；命令封装 `src/lib/sync/api.ts`。仅本地 Vault 可配置（`remote:` 句柄显示提示）。

## 边界（后续分期）

自动同步（watcher debounce + 定时）、状态栏指示、孤儿 blob GC、E2EE、官方托管凭据 provider 均未实现，见设计草稿分期表。
