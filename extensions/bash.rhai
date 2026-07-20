// bash: run a shell command with safety guards.

fn contains_blocked(blocked, binary) {
    let i = 0;
    while i < blocked.len() {
        if blocked[i] == binary {
            return true;
        }
        i += 1;
    }
    return false;
}

fn allowlist_contains(allowlist, binary) {
    let i = 0;
    while i < allowlist.len() {
        if allowlist[i] == binary {
            return true;
        }
        i += 1;
    }
    return false;
}

fn register(api) {
    let blocked = [
        "rm", "dd", "mkfs", "parted", "fastboot", "shutdown", "reboot",
        "sudo", "curl", "wget", "chmod", "chown",
    ];

    api.register_tool(
        "bash",
        "run a shell command (subject to blocklist and safe-mode allowlist)",
        `{
            "type": "object",
            "properties": {
                "command": { "type": "string" }
            },
            "required": ["command"]
        }`,
        |args| {
            let raw = args.command;
            if type_of(raw) != "string" {
                throw "command required";
            }
            if raw.len() == 0 {
                throw "command required";
            }
            raw.trim();
            if raw.len() == 0 {
                throw "command required";
            }
            if raw.contains("`") {
                throw "command substitution is not allowed";
            }
            if raw.contains("$(") {
                throw "command substitution is not allowed";
            }
            if raw.contains("rm -rf") {
                throw "rm -rf is not allowed";
            }
            let words = raw.split(" ");
            if words.len() == 0 {
                throw "command required";
            }
            let binary = words[0];
            if contains_blocked(blocked, binary) {
                throw binary + " is blocked";
            }
            let host = api.host;
            let full = host.config_get("bash_full");
            if !full {
                let i = 0;
                while i < words.len() {
                    if words[i].contains(">") {
                        throw "shell redirection is not allowed in safe mode";
                    }
                    i += 1;
                }
                let allowlist = host.config_get("bash_allowlist");
                if !allowlist_contains(allowlist, binary) {
                    throw binary + " is not in the allowlist";
                }
            }
            let result = host.run_command("sh", ["-c", raw]);
            if result.status != 0 {
                throw "exit " + result.status + ": " + result.stderr;
            }
            return result.stdout;
        }
    );
}