#!/usr/bin/env node
// Minimal ACP agent used to verify the node's ACP executor without a real
// agent. Speaks ACP v1 over stdio (newline-delimited JSON-RPC 2.0):
//   initialize -> session/new -> session/prompt (streams two message chunks).
// Not a real agent; it echoes the prompt back.
const readline = require("node:readline");

const rl = readline.createInterface({ input: process.stdin });
const sessionId = "mock-session";

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

rl.on("line", (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  let msg;
  try {
    msg = JSON.parse(trimmed);
  } catch {
    return;
  }
  const { id, method, params } = msg;

  if (method === "initialize") {
    send({
      jsonrpc: "2.0",
      id,
      result: {
        protocolVersion: 1,
        agentCapabilities: {},
        agentInfo: { name: "mock-acp", version: "1.0.0" },
      },
    });
  } else if (method === "session/new") {
    send({ jsonrpc: "2.0", id, result: { sessionId } });
  } else if (method === "session/prompt") {
    const text = ((params && params.prompt) || []).map((p) => p.text || "").join(" ");
    for (const part of ["mock-acp: ", text]) {
      send({
        jsonrpc: "2.0",
        method: "session/update",
        params: {
          sessionId,
          update: {
            sessionUpdate: "agent_message_chunk",
            content: { type: "text", text: part },
          },
        },
      });
    }
    send({ jsonrpc: "2.0", id, result: { stopReason: "end_turn" } });
  } else if (id !== undefined) {
    send({ jsonrpc: "2.0", id, result: {} });
  }
});
