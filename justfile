red := `tput setaf 1`
normal := `tput sgr0`
bold := `tput bold`
error := bold + red + "ERROR:" + normal

chooser := "grep -v choose | ${JUST_CHOOSER:-fzf --tmux}"

# Display this list of available commands
@list:
    just --justfile "{{ source_file() }}" --list

alias c := choose
# Open an interactive chooser of available commands
[no-exit-message]
@choose:
    just --justfile "{{ source_file() }}" --chooser "{{ chooser }}" --choose 2>/dev/null

alias e := edit
# Edit the justfile
@edit:
    $EDITOR "{{ justfile() }}"

[no-exit-message]
@_check-serve-requirements:
    command -v python3 >/dev/null || (echo "{{ error }} python3 missing" && exit 1)
    command -v bore >/dev/null || (echo "{{ error }} bore missing (https://github.com/ekzhang/bore)" && exit 1)

[doc("Serve the plugin")]
serve port="6969": _check-serve-requirements
    #!/usr/bin/env bash
    target="$(find target -depth 3 -name "*.wasm")"
    target_dir="$(dirname "$target")"
    target_name="$(basename "$target")"
    python3 -m http.server {{ port }} --directory "$target_dir" &>/dev/null &
    server_pid="$!"
    sleep 1
    trap "kill $server_pid; exit" SIGINT
    exec 3< <(bore local 6969 --to bore.pub)
    while read -r line; do
        echo $line | grep -o "listening at bore.pub:.*" | cut -d: -f2 | { read port; test -n "$port" && echo "serving plugin @ http://bore.pub:$port/$target_name"; }
    done <&3
