// write: create or overwrite a file at a relative path inside the workspace.

fn register(api) {
    api.register_tool(
        "write",
        "create or overwrite a file at a relative path inside the workspace",
        `{
            "type": "object",
            "properties": {
                "path":    { "type": "string", "description": "relative path to write" },
                "content": { "type": "string", "description": "content to write" }
            },
            "required": ["path", "content"]
        }`,
        |args| {
            let path = args.path;
            if type_of(path) != "string" {
                throw "path required";
            }
            let content = args.content;
            if type_of(content) != "string" {
                throw "content required";
            }
            let host = api.host;
            host.write_file(path, content);
            return "wrote " + content.bytes + " bytes to " + path;
        }
    );
}