use clap::Parser;
use new_string_template::template::Template;
use notify_debouncer_mini::{new_debouncer, notify::*, DebouncedEventKind};
use std::thread;
use std::time::Duration;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use subprocess::Exec;

#[derive(Parser)]
struct Cli {
    /// Scan duration in miliseconds
    #[arg(short, long, default_value = "500")]
    duration: u64,
    /// Watch in Recursive Mode
    #[arg(short, long, action)]
    recursive: bool,
    /// Run commands on Async
    #[arg(short, long, action)]
    r#async: bool,
    /// List paths to watch
    #[arg(short, long, default_value = "Change Detected: {path}")]
    template: String,
    /// List paths to watch
    #[arg(num_args(1..), required(true))]
    watch: Vec<PathBuf>,
    /// Command to run
    #[arg(num_args(0..), last(true))]
    command: Vec<String>,
}

fn template_vars(path: &PathBuf) -> HashMap<&'static str, String> {
    let mut map: HashMap<&str, String> = HashMap::new();
    map.insert("path", format!("{:?}", path));
    map.insert(
        "dir",
        format!("{:?}", path.parent().unwrap_or(Path::new("/"))),
    );
    map.insert(
        "name.ext",
        format!("{:?}", path.file_name().unwrap_or_default()),
    );
    map.insert("ext", format!("{:?}", path.extension().unwrap_or_default()));
    map.insert(
        "name",
        format!("{:?}", path.file_stem().unwrap_or_default()),
    );
    // TODO Relative path
    // map.insert("rpath", path.rel);
    map
}

fn main() {
    let args = Cli::parse();
    let cng_templ = Template::new(args.template);
    let cmd_templ = Template::new(args.command.join(" "));
    let (tx, rx) = std::sync::mpsc::channel();

    let mut debouncer = new_debouncer(Duration::from_millis(args.duration), None, tx).unwrap();

    let rm = if args.recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    let watcher = debouncer.watcher();
    for path in args.watch {
        println!("watching: {:?}", path);
        watcher.watch(path.as_ref(), rm).unwrap();
    }

    for res in rx {
        match res {
            Ok(events) => events.iter().for_each(|event| match event.kind {
                DebouncedEventKind::Any => {
                    let mut map = template_vars(&event.path);
                    map.insert("event", format!("{:?}", event));
                    println!("{}", cng_templ.render_nofail(&map));
                    let cmd = cmd_templ.render_nofail(&map);
                    if args.r#async {
                        thread::spawn(|| {
                            Exec::shell(cmd).join().unwrap();
                        });
                    } else {
                        Exec::shell(cmd).join().unwrap();
                    }
                }
                _ => (),
            }),
            Err(errors) => errors.iter().for_each(|e| println!("Error {:?}", e)),
        }
    }
}
