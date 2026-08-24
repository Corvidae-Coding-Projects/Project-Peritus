//! Deterministic current-JSONL scenarios selected through the configured model.

use super::trace;

const SUCCESS: &str = "{\"type\":\"thread.started\",\"thread_id\":\"fake-thread\"}\n{\"type\":\"turn.started\"}\n{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"{\\\"content\\\":\\\"routed\\\",\\\"tool_calls\\\":[]}\"}}\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":12,\"cached_input_tokens\":2,\"output_tokens\":5,\"total_tokens\":17}}\n";
const TOOL: &str = "{\"type\":\"thread.started\",\"thread_id\":\"fake-thread\"}\n{\"type\":\"turn.started\"}\n{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"{\\\"content\\\":\\\"\\\",\\\"tool_calls\\\":[{\\\"name\\\":\\\"lookup\\\",\\\"arguments_json\\\":\\\"{\\\\\\\"value\\\\\\\":\\\\\\\"fragmented-arguments-for-host-tool\\\\\\\"}\\\"}]}\"}}\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":12,\"output_tokens\":5,\"total_tokens\":17}}\n";
const ORDERED: &str = "{\"type\":\"thread.started\",\"thread_id\":\"fake-thread\"}\n{\"type\":\"item.started\",\"item\":{\"type\":\"reasoning\"}}\n{\"type\":\"item.started\",\"item\":{\"type\":\"reasoning\"}}\n{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"{\\\"content\\\":\\\"ordered\\\",\\\"tool_calls\\\":[]}\"}}\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":3,\"output_tokens\":2,\"total_tokens\":5}}\n";

pub(super) fn output(model: &str, invocation: u64) -> (&'static str, i32) {
    if model.contains("malformed") {
        return ("{not-json}\n", 0);
    }
    if model.contains("incomplete") {
        return (
            "{\"type\":\"thread.started\",\"thread_id\":\"fake-thread\"}\n{\"type\":\"item.started\",\"item\":{\"type\":\"reasoning\"}}\n",
            0,
        );
    }
    if model.contains("interruption") {
        return (
            "{\"type\":\"thread.started\",\"thread_id\":\"fake-thread\"}\n{\"type\":\"item.started\",\"item\":{\"type\":\"reasoning\"}}\n",
            7,
        );
    }
    if model.contains("ambiguous") {
        return ("", 7);
    }
    if model.contains("authentication") {
        return (
            "{\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"untrusted secret detail\"}}\n",
            1,
        );
    }
    if model.contains("native-tool") {
        return (
            "{\"type\":\"item.started\",\"item\":{\"type\":\"command_execution\"}}\n{\"type\":\"turn.completed\",\"usage\":{}}\n",
            0,
        );
    }
    if model.contains("rate-limit") && invocation == 1 {
        trace::record("failure-rate-limited-250");
        return ("{\"type\":\"error\",\"message\":\"synthetic first turn\"}\n", 1);
    }
    if model.contains("transient") && invocation == 1 {
        trace::record("failure-transient-0");
        return ("{\"type\":\"turn.failed\",\"message\":\"synthetic first turn\"}\n", 1);
    }
    if model.contains("ordered") {
        trace::record("duplicate");
        return (ORDERED, 0);
    }
    if model.contains("tool") || model.contains("capability") {
        return (TOOL, 0);
    }
    (SUCCESS, 0)
}
