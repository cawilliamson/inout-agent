// glob: recursively list files and directories matching a simple glob pattern.

fn match_glob(pattern, name) {
    let p = pattern.to_chars();
    let n = name.to_chars();
    return glob_rec(p, 0, n, 0);
}

fn glob_rec(p, pi, n, ni) {
    let plen = p.len();
    let nlen = n.len();
    if pi == plen {
        return ni == nlen;
    }
    let pc = p[pi];
    if pc == '*' {
        let k = ni;
        while k <= nlen {
            if glob_rec(p, pi + 1, n, k) {
                return true;
            }
            k += 1;
        }
        return false;
    }
    if ni == nlen {
        return false;
    }
    if pc == '?' {
        return glob_rec(p, pi + 1, n, ni + 1);
    }
    if pc == n[ni] {
        return glob_rec(p, pi + 1, n, ni + 1);
    }
    return false;
}

fn register(api) {
    api.register_tool(
        "glob",
        "recursively list files and directories matching a simple glob pattern",
        `{
            "type": "object",
            "properties": {
                "path":    { "type": "string", "description": "relative directory under the jail root" },
                "pattern": { "type": "string", "description": "glob pattern supporting * and ?, matched against entry names" }
            },
            "required": ["path"]
        }`,
        |args| {
            let rel_path = "";
            if args.contains("path") {
                if type_of(args.path) == "string" {
                    rel_path = args.path;
                }
            }
            let pattern = "*";
            if args.contains("pattern") {
                if type_of(args.pattern) == "string" {
                    pattern = args.pattern;
                }
            }
            let host = api.host;
            let entries = host.walk_dir(rel_path);
            let out = "";
            let i = 0;
            let elen = entries.len();
            while i < elen {
                let entry = entries[i];
                let segs = entry.split("/");
                let last = segs[segs.len() - 1];
                if match_glob(pattern, last) {
                    if out.len() > 0 {
                        out += "\n";
                    }
                    out += entry;
                }
                i += 1;
            }
            return out;
        }
    );
}