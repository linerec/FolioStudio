//! A project-scoped MCP stdio adapter, or (`--attach`) one attached to the running Folio app. stdout is reserved for JSON-RPC.
mod mcp;

fn main() {
    if let Err(error) = run() {
        eprintln!("folio-mcp: {error}");
        std::process::exit(1);
    }
}

const USAGE: &str = "usage: folio-mcp <project-folder> | folio-mcp --attach [--connection <file>] [<project-folder>]";

fn run() -> Result<(), String> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--attach") {
        args.remove(0);
        let mut file = None;
        if args.first().is_some_and(|a| a == "--connection") {
            if args.len() < 2 { return Err(USAGE.into()); }
            file = Some(std::path::PathBuf::from(args.remove(1)));
            args.remove(0);
        }
        if args.len() > 1 { return Err(USAGE.into()); }
        let file = file.or_else(mcp::client::default_file).ok_or("no user data folder: pass --connection <file>")?;
        let connected = mcp::client::connect(&file);
        // not attached: the project folder, if given, still works in this process (the client is told why)
        let fallback = match (&connected, args.first()) {
            (Err(_), Some(folder)) => Some(mcp::document::Engine::new(std::path::Path::new(folder))?),
            _ => None,
        };
        let server = mcp::Server::attached(connected, fallback);
        return mcp::serve_with(std::io::stdin().lock(), std::io::stdout().lock(), server).map_err(|e| e.to_string());
    }
    if args.len() != 1 {
        return Err(USAGE.into());
    }
    let engine = mcp::document::Engine::new(std::path::Path::new(&args[0]))?;
    mcp::serve(std::io::stdin().lock(), std::io::stdout().lock(), engine)
        .map_err(|e| e.to_string())
}
