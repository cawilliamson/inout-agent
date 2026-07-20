// grep: return lines from a file that contain a substring pattern.

fn register(api) {
    api.register_tool(
        "grep",
        "return lines from a file that contain a substring pattern",
        `{
            "type": "object",
            "properties": {
                "path":           { "type": "string", "description": "relative path to the file" },
                "pattern":        { "type": "string", "description": "substring to search for" },
                "case_sensitive": { "type": "boolean", "description": "match case sensitively (default true)" }
            },
            "required": ["path", "pattern"]
        }`,
        |args| {
            let path = args.path;
            if type_of(path) != "string" {
                throw "path required";
            }
            let pattern = args.pattern;
            if type_of(pattern) != "string" {
                throw "pattern required";
            }
            let case_sensitive = true;
            if args.contains("case_sensitive") {
                if type_of(args.case_sensitive) == "bool" {
                    case_sensitive = args.case_sensitive;
                }
            }
            let host = api.host;
            let content = host.read_file(path);
            let lines = content.split("\n");
            let needle = pattern;
            if !case_sensitive {
                needle = pattern.to_lower();
            }
            let out = "";
            let i = 0;
            let lines_len = lines.len();
            while i < lines_len {
                let line = lines[i];
                let hay = line;
                if !case_sensitive {
                    hay = line.to_lower();
                }
                if hay.contains(needle) {
                    if out.len() > 0 {
                        out += "\n";
                    }
                    out += line;
                }
                i += 1;
            }
            return out;
        }
    );
}