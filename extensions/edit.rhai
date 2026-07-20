// edit: replace the first occurrence of old_string with new_string in a file.

fn register(api) {
    api.register_tool(
        "edit",
        "replace the first occurrence of old_string with new_string in a file",
        `{
            "type": "object",
            "properties": {
                "path":       { "type": "string", "description": "relative path to the file inside the repo" },
                "old_string": { "type": "string", "description": "exact text to replace" },
                "new_string": { "type": "string", "description": "replacement text" }
            },
            "required": ["path", "old_string", "new_string"]
        }`,
        |args| {
            let path = args.path;
            if type_of(path) != "string" {
                throw "path required";
            }
            let old_string = args.old_string;
            if type_of(old_string) != "string" {
                throw "old_string required";
            }
            let new_string = args.new_string;
            if type_of(new_string) != "string" {
                throw "new_string required";
            }
            let host = api.host;
            let content = host.read_file(path);
            let idx = content.index_of(old_string);
            if idx < 0 {
                throw "old_string not found";
            }
            let before = content.sub_string(0, idx);
            let after_start = idx + old_string.len();
            let after_len = content.len() - after_start;
            let after = content.sub_string(after_start, after_len);
            let updated = before + new_string + after;
            host.write_file(path, updated);
            return "replaced " + old_string + " -> " + new_string + " in " + path;
        }
    );
}