#!/bin/sh
# rustsmith live seat dispatcher.
#
# Seat command for RUSTSMITH_SEAT_CMD_<ARCHITECT|VERIFIER|PERFORMANCE|SCOPE>:
# reads the seat prompt on stdin (carries `seat:`, `model:`, `provider:`
# lines from the configured [models] table), routes to the real model for
# that provider, and prints a bare JSON {"stance":...,"reasoning":...}
# object on stdout (the only bytes the WorkerSeatDriver parses).
#
# Dispatch (per config/live.toml route table):
#   anthropic-* provider -> Claude Code wrapper (`claude -p`, result field)
#   meta-* provider      -> omp --model muse-code/<model>
#   openai-* provider    -> omp --model openai-codex/<model>
# A model value already containing `/` is used verbatim as the omp spec.
#
# Any model/transport failure exits nonzero (fail-closed: the seat errors,
# it never approves by silence).
set -eu

prompt_file=$(mktemp)
trap 'rm -f "$prompt_file"' EXIT INT TERM
cat > "$prompt_file"

field() {
    sed -n "s/^$1: *//p" "$prompt_file" | head -n 1 | tr -d '\r'
}

model=$(field model)
provider=$(field provider)
seat=$(field seat)

if [ -z "$model" ] || [ -z "$provider" ]; then
    echo "seat dispatch ($seat): prompt carries no model/provider identity" >&2
    exit 1
fi

instruction='You are a rustsmith council reviewer. Critique ONLY the question and artifact above; you see no other seats positions. Reply with ONLY a bare JSON object, no code fences, no other text: {"stance": "approve" or "reject", "reasoning": "one paragraph"}. Approve only if the proposal is sound as stated; otherwise reject with the reason.'
full_prompt="$(cat "$prompt_file")
$instruction"

case "$provider" in
    anthropic*)
        # Claude Code wrapper: `claude -p --output-format json` envelopes the
        # answer; the model's text lives in the result field, which must be
        # the bare JSON verdict object.
        timeout 600 claude -p --model "$model" --output-format json -- "$full_prompt" \
            | jq -r '.result // error("claude JSON has no result field")'
        ;;
    meta*|openai*)
        case "$model" in
            */*) spec="$model" ;;
            *) case "$provider" in
                   meta*) spec="muse-code/$model" ;;
                   *) spec="openai-codex/$model" ;;
               esac ;;
        esac
        timeout 600 omp -p --model "$spec" --no-session -- "$full_prompt"
        ;;
    *)
        echo "seat dispatch ($seat): unknown provider '$provider'" >&2
        exit 1
        ;;
esac
