//! An external SDK consumer; generated services remain unmodified.
fn main() {
    let mut args = std::env::args_os().skip(1);
    let first = args.next().expect("project directory");
    let checkpoint = first == "--pause-after-workspace";
    let project = if checkpoint {
        args.next().expect("project directory")
    } else {
        first
    };
    let result = std::thread::Builder::new()
        .name("web-consumer-generation".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || -> Result<(), String> {
            let project = std::path::Path::new(&project);
            if checkpoint {
                use std::io::Write;
                auto_web::rust_workspace::try_ensure_shared_workspace(project)
                    .map_err(|e| e.to_string())?;
                println!("WEB_CONSUMER_WORKSPACE_CHECKPOINT");
                std::io::stdout().flush().unwrap();
                loop {
                    std::thread::park();
                }
            }
            auto_web::generate_api(project, "rust").map_err(|e| e.to_string())?;
            if !auto_web::backend_generation_is_fresh(project) {
                return Err("new Web generation is not fresh".into());
            }
            Ok(())
        })
        .expect("generation thread")
        .join()
        .expect("generation worker panicked");
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
    println!("Web public generation succeeded and freshness verified");
}
