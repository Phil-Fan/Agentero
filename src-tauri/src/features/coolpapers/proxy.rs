//! Sandboxed papers.cool proxy for the 广场 Cool Papers panel.
//!
//! The site marks nearly every link `target="_blank"`, which inside a plain
//! cross-origin iframe either spawns a separate window or silently does nothing,
//! and leaves no way to go back. Serving it through our own scheme makes the
//! frame same-origin, so we can retarget links to navigate in place and report
//! each navigation to the panel for a real history stack.
//!
//! The upstream origin is hardcoded: this proxy must never become an open relay.

use tauri::http::{header, Response, StatusCode};

const ORIGIN: &str = "https://papers.cool";
const USER_AGENT: &str = "agentero/0.6 (+https://github.com/poco-ai/agentero)";

/// Reports navigations to the panel (for Back / Forward), hands off links that
/// leave our origin, and adds an `[入库]` action to every paper row.
const NAV_BRIDGE: &str = r##"<style>
.title-import { color: #0a7a5a; cursor: pointer; }
.title-import[data-state="pending"] { color: #999; cursor: default; }
.title-import[data-state="done"] { color: #888; cursor: default; }
</style>
<script>
(function () {
  try {
    // Only the panel's own frame talks to the app. A nested frame (the pdf.js
    // viewer) has a same-origin, readable parent — leave those uninstrumented.
    try {
      if (parent !== window && parent.location.href) return;
    } catch (e) {}
    var post = function (message) {
      message.source = "agentero-plaza";
      parent.postMessage(message, "*");
    };
    var send = function () {
      post({ path: location.pathname + location.search });
    };
    send();
    window.addEventListener("pageshow", send);
    document.addEventListener(
      "click",
      function (event) {
        var anchor = event.target && event.target.closest
          ? event.target.closest("a[href]")
          : null;
        if (!anchor) return;
        var raw = anchor.getAttribute("href");
        if (!raw || raw.charAt(0) === "#") return;
        if (raw.toLowerCase().indexOf("javascript:") === 0) return;
        var url;
        try {
          url = new URL(anchor.href, location.href);
        } catch (e) {
          return;
        }
        if (url.origin === location.origin) return;
        event.preventDefault();
        post({ external: url.href });
      },
      true
    );

    // ---- [入库] ----------------------------------------------------------
    // The `#N` index anchor is the upstream landing page on every branch
    // (arxiv.org / OJS / OpenReview / ACL Anthology), which is what the
    // importer resolves. No href on our own anchor, so the interceptor
    // above ignores it.
    var upstreamUrl = function (panel) {
      var index = panel.querySelector(".index");
      var anchor = index && index.closest ? index.closest("a[href]") : null;
      if (!anchor) anchor = panel.querySelector("h2.title a[href]");
      return anchor ? anchor.href : null;
    };
    var setState = function (button, state, label) {
      button.dataset.state = state;
      button.textContent = label;
    };
    var decorate = function (panel) {
      if (!panel || panel.querySelector(".title-import")) return;
      var title = panel.querySelector("h2.title");
      if (!title) return;
      var url = upstreamUrl(panel);
      if (!url) return;
      var button = document.createElement("a");
      button.className = "title-import notranslate";
      button.textContent = "[入库]";
      button.title = "导入到我的论文库";
      button.dataset.paperId = panel.id;
      button.dataset.url = url;
      title.appendChild(document.createTextNode(" "));
      title.appendChild(button);
    };
    var decorateAll = function (root) {
      var panels = (root || document).querySelectorAll(".panel.paper");
      for (var i = 0; i < panels.length; i++) decorate(panels[i]);
    };
    // Injected into <head>, so the body does not exist yet: sweeping now would
    // find no rows and `.papers` would be null (observer never attaches).
    var start = function () {
      decorateAll(document);
      // Infinite scroll appends more rows.
      var papers = document.querySelector(".papers");
      if (papers && window.MutationObserver) {
        new MutationObserver(function (records) {
          for (var i = 0; i < records.length; i++) {
            var added = records[i].addedNodes;
            for (var j = 0; j < added.length; j++) {
              var node = added[j];
              if (node.nodeType !== 1) continue;
              if (node.classList && node.classList.contains("paper")) decorate(node);
              else decorateAll(node);
            }
          }
        }).observe(papers, { childList: true });
      }
    };
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", start);
    } else {
      start();
    }

    document.addEventListener("click", function (event) {
      var button = event.target && event.target.closest
        ? event.target.closest(".title-import")
        : null;
      if (!button) return;
      event.preventDefault();
      event.stopPropagation();
      if (button.dataset.state) return;
      var titleLink = document.getElementById("title-" + button.dataset.paperId);
      setState(button, "pending", "[入库中]");
      post({
        importPaper: {
          id: button.dataset.paperId,
          url: button.dataset.url,
          title: titleLink ? titleLink.textContent.trim() : null
        }
      });
    });

    window.addEventListener("message", function (event) {
      var data = event.data;
      if (!data || data.source !== "agentero-plaza-host") return;
      if (typeof data.importedId !== "string") return;
      var panel = document.getElementById(data.importedId);
      var button = panel ? panel.querySelector(".title-import") : null;
      if (!button) return;
      if (data.ok) {
        button.dataset.state = "done";
        button.textContent = "[已入库]";
      } else {
        delete button.dataset.state;
        button.textContent = "[入库]";
      }
    });
  } catch (e) {}
})();
</script>"##;

fn response(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .body(body)
        .expect("valid cool papers proxy response")
}

/// Whether a body is a full page rather than an XHR fragment.
///
/// `togglePdf` / `toggleKimi` fetch fragments that are *also* `text/html` (the
/// Kimi analysis, and the bare star counter) and write the response text
/// straight into the DOM. Injecting into those renders the bridge source as
/// visible text, so only real documents may be touched.
fn looks_like_document(body: &str) -> bool {
    let head: String = body
        .trim_start_matches('\u{feff}')
        .trim_start()
        .chars()
        .take(16)
        .collect::<String>()
        .to_ascii_lowercase();
    head.starts_with("<!doctype") || head.starts_with("<html")
}

/// Keep every click inside the frame and keep internal links on this scheme.
fn rewrite_html(html: &str) -> String {
    let retargeted = html
        .replace("target=\"_blank\"", "target=\"_self\"")
        .replace("target='_blank'", "target='_self'")
        // Absolute self-links would leave the proxy scheme behind.
        .replace("https://papers.cool/", "/");
    match retargeted.find("</head>") {
        Some(_) => retargeted.replacen("</head>", &format!("{NAV_BRIDGE}</head>"), 1),
        None => format!("{NAV_BRIDGE}{retargeted}"),
    }
}

pub fn handle(request: tauri::http::Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
    let path = request.uri().path().to_string();
    if !path.starts_with('/') || path.contains("..") {
        responder.respond(response(
            StatusCode::BAD_REQUEST,
            "text/plain",
            b"invalid cool papers path".to_vec(),
        ));
        return;
    }
    let query = request
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();
    let url = format!("{ORIGIN}{path}{query}");

    tauri::async_runtime::spawn(async move {
        let result = async {
            let client = crate::features::network::client_builder()
                .user_agent(USER_AGENT)
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()?;
            let remote = client.get(url).send().await?;
            let status =
                StatusCode::from_u16(remote.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let content_type = remote
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            let bytes = remote.bytes().await?;
            let body = if content_type.starts_with("text/html") {
                match String::from_utf8(bytes.to_vec()) {
                    Ok(text) if looks_like_document(&text) => rewrite_html(&text).into_bytes(),
                    // XHR fragments (Kimi analysis, star counter) go through as-is.
                    _ => bytes.to_vec(),
                }
            } else {
                bytes.to_vec()
            };
            Ok::<_, reqwest::Error>((status, content_type, body))
        }
        .await;

        match result {
            Ok((status, content_type, body)) => {
                responder.respond(response(status, &content_type, body))
            }
            Err(error) => {
                log::warn!(target: "agentero::coolpapers", "cool papers proxy request failed: {error}");
                responder.respond(response(
                    StatusCode::BAD_GATEWAY,
                    "text/plain",
                    b"Cool Papers unavailable".to_vec(),
                ));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retargets_blank_links_so_they_open_in_frame() {
        let out = rewrite_html("<a href=\"/arxiv/cs.AI\" target=\"_blank\">x</a>");
        assert!(out.contains("target=\"_self\""));
        assert!(!out.contains("_blank"));
    }

    #[test]
    fn rewrites_absolute_self_links_to_stay_on_the_proxy() {
        let out = rewrite_html("<a href=\"https://papers.cool/arxiv/2608.13558\">x</a>");
        assert!(out.contains("href=\"/arxiv/2608.13558\""));
        assert!(!out.contains("https://papers.cool/"));
    }

    #[test]
    fn leaves_third_party_links_alone() {
        let out = rewrite_html("<a href=\"https://arxiv.org/abs/1706.03762\">x</a>");
        assert!(out.contains("https://arxiv.org/abs/1706.03762"));
    }

    #[test]
    fn injects_nav_bridge_before_head_close() {
        let out = rewrite_html("<html><head><title>t</title></head><body></body></html>");
        assert!(out.contains("agentero-plaza"));
        let script = out.find("agentero-plaza").expect("bridge present");
        let head_end = out.find("</head>").expect("head close present");
        assert!(script < head_end);
    }

    #[test]
    fn injects_nav_bridge_even_without_a_head() {
        let out = rewrite_html("<p>fragment</p>");
        assert!(out.contains("agentero-plaza"));
        assert!(out.contains("<p>fragment</p>"));
    }

    #[test]
    fn ships_the_import_affordance() {
        let out = rewrite_html("<!DOCTYPE html><html><head></head><body></body></html>");
        assert!(out.contains("title-import"));
        assert!(out.contains("importPaper"));
        // Settled from the app so a row can show 已入库 / stay retryable.
        assert!(out.contains("agentero-plaza-host"));
    }

    /// The bridge lands in `<head>`, where the body does not exist yet: decorating
    /// rows there finds nothing and leaves the scroll observer unattached.
    #[test]
    fn defers_row_decoration_until_the_dom_exists() {
        assert!(NAV_BRIDGE.contains("DOMContentLoaded"));
        assert!(NAV_BRIDGE.contains("document.readyState"));
    }

    #[test]
    fn treats_full_pages_as_documents() {
        assert!(looks_like_document("<!DOCTYPE html>\n<html>\n<head>"));
        assert!(looks_like_document("\n  <!doctype html><html>"));
        assert!(looks_like_document("<html lang=\"en\">"));
    }

    /// `toggleKimi` writes this body into the DOM; a bridge here shows as text.
    #[test]
    fn treats_kimi_fragment_as_non_document() {
        let fragment = "<p class=\"faq-q\"><strong>Q1</strong>: 试图解决什么问题？</p>\n\n<div class=\"faq-a\">\n\n答案\n\n</div>";
        assert!(!looks_like_document(fragment));
    }

    /// `POST /star` answers with a bare count — the stray `0` users saw.
    #[test]
    fn treats_star_counter_as_non_document() {
        assert!(!looks_like_document("1060"));
        assert!(!looks_like_document("0"));
        assert!(!looks_like_document(""));
    }
}
