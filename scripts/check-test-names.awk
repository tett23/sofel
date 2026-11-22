# Parse only test declarations, keeping ADR references local to each name.
BEGIN {
    while ((getline line < index_file) > 0) {
        split(line, fields, " ")
        declared[fields[1], fields[2]] = 1
    }
    close(index_file)
    while ((getline entry < exceptions_file) > 0) {
        if (entry ~ /^#/ || entry ~ /^[[:space:]]*$/) continue
        count = split(entry, fields, "\t")
        if (count != 3 || fields[1] == "" || fields[2] == "" || fields[3] ~ /^[[:space:]]*$/) {
            print exceptions_file ": invalid exception row: " entry > "/dev/stderr"
            failed = 1
            continue
        }
        key = fields[1] SUBSEP fields[2]
        if (key in exceptions) {
            print exceptions_file ": duplicate exception: " fields[1] " " fields[2] > "/dev/stderr"
            failed = 1
        }
        exceptions[key] = 1
    }
    close(exceptions_file)
    while ((getline path < files_file) > 0) {
        line_number = 0
        pending = 0
        while ((getline source < path) > 0) {
            line_number++
            inspect(source)
        }
        close(path)
    }
    close(files_file)
    for (key in exceptions) {
        if (!(key in used)) {
            split(key, fields, SUBSEP)
            print exceptions_file ": missing test: " fields[1] " " fields[2] > "/dev/stderr"
            failed = 1
        }
    }
    exit failed ? 1 : 0
}
function exempt(name, key) {
    key = path SUBSEP name
    if (!(key in exceptions)) return 0
    used[key] = 1
    return 1
}
function reject(name, reason) {
    print path ":" line_number ": " name ": " reason > "/dev/stderr"
    failed = 1
}
function reference(name, adr, condition) {
    if (!declared[adr, condition]) reject(name, "unknown ADR invariant or acceptance condition " adr " " condition)
}
function rust_name(name, count, tokens, i, adr) {
    if (exempt(name) || name !~ /adr[0-9]{4}/) return
    if (name !~ /^adr[0-9]{4}_[ia][1-9][0-9]*(_[ia][1-9][0-9]*|_adr[0-9]{4}_[ia][1-9][0-9]*)*_[A-Za-z0-9_]+$/) {
        reject(name, "invalid Rust test name")
        return
    }
    count = split(name, tokens, "_")
    adr = substr(tokens[1], 4)
    for (i = 2; i <= count; i++) {
        if (tokens[i] ~ /^adr[0-9]{4}$/) adr = substr(tokens[i], 4)
        else if (tokens[i] ~ /^[ia][1-9][0-9]*$/) reference(name, adr, toupper(tokens[i]))
        else break
    }
}
function shell_name(name, prefix, count, tokens, i, adr, words) {
    if (exempt(name) || name !~ /ADR [0-9]{4}/) return
    if (name !~ /^ADR [0-9]{4} [IA][1-9][0-9]*(, ([IA][1-9][0-9]*|ADR [0-9]{4} [IA][1-9][0-9]*))*: .+/) {
        reject(name, "invalid shell test name")
        return
    }
    prefix = name
    sub(/: .*/, "", prefix)
    count = split(prefix, tokens, ", ")
    for (i = 1; i <= count; i++) {
        if (tokens[i] ~ /^ADR /) {
            split(tokens[i], words, " ")
            adr = words[2]
            reference(name, adr, words[3])
        } else reference(name, adr, tokens[i])
    }
}
function inspect(source, name, text) {
    if (path ~ /\.rs$/) {
        if (source ~ /^[[:space:]]*#\[test\]/) pending = 1
        if (pending && match(source, /fn[[:space:]]+[A-Za-z_][A-Za-z0-9_]*/)) {
            name = substr(source, RSTART, RLENGTH)
            sub(/^fn[[:space:]]+/, "", name)
            rust_name(name)
            pending = 0
        }
    }
    if (path ~ /scripts\/tests\/.*\.sh$/ && source !~ /^[[:space:]]*#/ &&
        source ~ /(^|[[:space:];|&])(fail|check|assert|echo|reject|expect)[[:space:]]/) {
        quoted_names(source)
    }
}
function quoted_names(text, i, quote, character, name) {
    quote = ""
    name = ""
    for (i = 1; i <= length(text); i++) {
        character = substr(text, i, 1)
        if (quote == "") {
            if (character == "\"" || character == "\047") quote = character
        } else if (character == quote) {
            shell_name(name)
            quote = ""
            name = ""
        } else if (character == "\\" && quote == "\"") {
            i++
            name = name substr(text, i, 1)
        } else name = name character
    }
}
