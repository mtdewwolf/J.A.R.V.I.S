// Ollama streaming adapter (ADR-0005 candidate).
//
// Ollama streams newline-delimited JSON for /api/chat with "stream": true.
// Individual models still require qualification; this only proves the
// transport mapping and cancellation behavior.

import { strictParse } from "../strict-json";
import type { InferenceProvider, InferenceRequest, RawEvent } from "../types";

export class OllamaProvider implements InferenceProvider {
  readonly id = "ollama";

  constructor(private readonly baseUrl = "http://127.0.0.1:11434") {}

  async *stream(request: InferenceRequest, signal: AbortSignal): AsyncIterable<RawEvent> {
    const body = {
      model: request.model.name,
      stream: true,
      options: { num_predict: request.maxOutputTokens },
      messages: [
        ...request.system.map((s) => ({ role: "system", content: s.content })),
        ...request.messages.map((m) => ({ role: m.role === "tool" ? "user" : m.role, content: m.content })),
      ],
      tools: request.tools.map((t) => ({
        type: "function",
        function: { name: t.name, description: (t.schema as { description?: string }).description ?? "", parameters: t.schema },
      })),
    };

    let response: Response;
    try {
      response = await fetch(`${this.baseUrl}/api/chat`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
        signal,
      });
    } catch (err) {
      yield {
        type: "error",
        payload: { code: "connection_error", message: err instanceof Error ? err.message : String(err) },
      };
      return;
    }

    if (!response.ok || !response.body) {
      yield { type: "error", payload: { code: "http_error", message: `ollama returned ${response.status}` } };
      return;
    }

    yield { type: "started", payload: { provider: this.id, model: request.model.name } };

    const decoder = new TextDecoder();
    let buffer = "";
    for await (const chunk of response.body) {
      buffer += decoder.decode(chunk, { stream: true });
      if (buffer.length > 131072) throw new Error("provider frame exceeds limit");
      let newline = buffer.indexOf("\n");
      while (newline !== -1) {
        const line = buffer.slice(0, newline).trim();
        buffer = buffer.slice(newline + 1);
        if (line) {
          for (const event of ollamaEventsFromLine(line)) yield event;
        }
        newline = buffer.indexOf("\n");
      }
      if (signal.aborted) return;
    }
    buffer += decoder.decode();
    if (buffer.trim()) for (const event of ollamaEventsFromLine(buffer.trim())) yield event;
  }
}

/** Map one NDJSON line into zero or one raw event. */
export function ollamaEventFromLine(line: string): RawEvent | null {
  let frame: Record<string, unknown>;
  try {
    frame = strictParse(line) as Record<string, unknown>;
  } catch {
    return null;
  }

  if (frame.error) {
    return { type: "error", payload: { code: "ollama_error", message: String(frame.error) } };
  }

  const message = frame.message as Record<string, unknown> | undefined;
  const content = message?.content;
  if (typeof content === "string" && content.length > 0) {
    return { type: "content.delta", payload: { text: content } };
  }

  const toolCalls = message?.tool_calls as Array<Record<string, unknown>> | undefined;
  if (Array.isArray(toolCalls) && toolCalls.length > 0) {
    const call = toolCalls[0];
    const fn = call.function as Record<string, unknown> | undefined;
    return {
      type: "tool_use.proposal",
      payload: {
        name: String(fn?.name ?? "workspace.read_text"),
        arguments: fn?.arguments ?? {},
      },
    };
  }

  if (frame.done === true) {
    return {
      type: "completed",
      payload: {
        stopReason: frame.done_reason === "length" ? "max_tokens" : "stop",
        usage: {
          inputTokens: Number(frame.prompt_eval_count ?? 0),
          outputTokens: Number(frame.eval_count ?? 0),
        },
      },
    };
  }
  return null;
}

/** Preserve all events when the final NDJSON frame also contains content/tools. */
export function ollamaEventsFromLine(line:string):RawEvent[] {
  const frame=strictParse(line) as Record<string,unknown>;
  const events:RawEvent[]=[];
  const first=ollamaEventFromLine(line);if(first)events.push(first);
  const message=frame.message as Record<string,unknown>|undefined;
  if(first?.type==='content.delta' && Array.isArray(message?.tool_calls) && message.tool_calls.length){
    const proposal=ollamaEventFromLine(JSON.stringify({...frame,done:false,message:{...message,content:''}}));if(proposal)events.push(proposal);
  }
  if(frame.done===true && first?.type!=='completed' && first?.type!=='error'){
    const completion=ollamaEventFromLine(JSON.stringify({...frame,message:{}}));if(completion)events.push(completion);
  }
  return events;
}
