// read: read a file from the repository, optionally selecting a line range.

fn join_lines(arr) {
    let out = "";
    let i = 0;
    let n = arr.len();
    while i < n {
        if out.len() > 0 {
            out += "\n";
        }
        out += arr[i];
        i += 1;
    }
    return out;
}

fn opt_int(args, key, def_val) {
    if args.contains(key) {
        let v = args[key];
        if type_of(v) == "i64" {
            if v > 0 {
                return v;
            }
        }
    }
    return def_val;
}

fn compute_end(start, limit, total) {
    if limit <= 0 {
        return total;
    }
    let e = start + limit;
    if e > total {
        return total;
    }
    return e;
}

fn register(api) {
    api.register_tool(
        "read",
        "read a file from the repository, optionally selecting a line range",
        `{
            "type": "object",
            "properties": {
                "path":  { "type": "string", "description": "relative path inside the repository" },
                "offset": { "type": "integer", "description": "1-based starting line (default 1)" },
                "limit":  { "type": "integer", "description": "maximum lines to return; 0 means all remaining lines" }
            },
            "required": ["path"]
        }`,
        |args| {
            let path = args.path;
            if type_of(path) != "string" {
                throw "path required";
            }
            let host = api.host;
            let content = host.read_file(path);
            let offset = opt_int(args, "offset", 1);
            let limit = opt_int(args, "limit", 0);
            let lines = content.split("\n");
            let total = lines.len();
            let start = offset - 1;
            if start >= total {
                return "";
            }
            let end = compute_end(start, limit, total);
            let out = [];
            let i = start;
            while i < end {
                out.push(lines[i]);
                i += 1;
            }
            return join_lines(out);
        }
    );
}