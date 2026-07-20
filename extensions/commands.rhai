// commands.rhai — core slash commands
//
// registers the base set of slash commands every agent needs:
// /help, /clear, /new, /model, /undo, /exit, /reload.

fn register(api) {
    // /help — list all registered commands
    api.register_command("help", "list available commands", |ctx| {
        #{
            message: "commands: /help /clear /new /model /undo /exit /reload /context /full",
            action: "",
        }
    });

    // /clear — clear conversation history
    api.register_command("clear", "clear conversation history", |ctx| {
        #{
            message: "history cleared",
            action: "clear_history",
        }
    });

    // /new — start a new conversation (alias for /clear)
    api.register_command("new", "start a new conversation", |ctx| {
        #{
            message: "new conversation started",
            action: "clear_history",
        }
    });

    // /model <name> — switch the active model
    api.register_command("model", "switch model: /model <name>", |ctx| {
        let m = ctx.args;
        if m.len() == 0 {
            #{
                message: "usage: /model <name>",
                action: "",
            }
        } else {
            #{
                message: "switching model to " + m,
                action: "set_model:" + m,
            }
        }
    });

    // /undo — drop the last turn
    api.register_command("undo", "drop the last turn", |ctx| {
        #{
            message: "last turn dropped",
            action: "undo_last_turn",
        }
    });

    // /exit — quit the agent
    api.register_command("exit", "exit the agent", |ctx| {
        #{
            message: "bye",
            action: "exit",
        }
    });

    // /reload — reload extensions
    api.register_command("reload", "reload extensions", |ctx| {
        #{
            message: "reloading extensions…",
            action: "reload",
        }
    });

    // /context — open context viewer
    api.register_command("context", "open context viewer", |ctx| {
        #{
            message: "opening context view…",
            action: "open_view:context",
        }
    });
}