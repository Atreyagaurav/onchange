use clap::Parser;
use colored::Colorize;
use config;
use new_string_template::template::Template;
use notify;
use notify::Watcher;
use std::time::{Duration, Instant};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use std::{env, thread};

use subprocess::Exec;

#[derive(Parser)]
struct Cli {
    /// Config file
    ///
    /// If none given it'll search from the following paths in order:
    ///
    /// - "/etc/onchange.toml"
    /// - "~/.config/onchange.toml"
    /// - ".onchange.toml"
    /// The later will overwrite the former if same config is present.
    #[arg(short, long)]
    config: Option<String>,
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
    #[arg(short, long, default_value = "{path}")]
    template: String,
    /// List paths to watch
    #[arg(num_args(1..), required(true))]
    watch: Vec<PathBuf>,
    /// Command to run
    #[arg(num_args(0..), last(true))]
    command: Vec<String>,
}

fn template_vars(path: &PathBuf, pwd: &PathBuf) -> HashMap<&'static str, String> {
    let mut map: HashMap<&str, String> = HashMap::new();
    map.insert(
        "name",
        path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    );
    map.insert(
        "ext",
        path.extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    );
    map.insert(
        "name.ext",
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    );
    map.insert("pwd", pwd.to_string_lossy().to_string());
    map.insert("path", path.to_string_lossy().to_string());
    map.insert(
        "rpath",
        pathdiff::diff_paths(path, pwd)
            .unwrap()
            .to_string_lossy()
            .to_string(),
    );
    let parent = path.parent().unwrap_or(Path::new("/"));
    map.insert("dir", parent.to_string_lossy().to_string());
    map.insert(
        "rdir",
        pathdiff::diff_paths(parent, pwd)
            .unwrap()
            .to_string_lossy()
            .to_string(),
    );
    map
}

fn get_config(conf: &Option<String>) -> Result<config::Config, String> {
    if let Some(conf_file) = conf {
        return config::Config::builder()
            .add_source(config::File::with_name(conf_file))
            .build()
            .map_err(|e| e.to_string());
    }
    config::Config::builder()
        .add_source(
            vec![
                PathBuf::from("/etc/onchange.toml"),
                PathBuf::from(format!(
                    "{}/.config/onchange.toml",
                    std::env::var("HOME").unwrap_or_default()
                )),
                PathBuf::from(".onchange.toml"),
            ]
            .iter()
            .filter(|f| f.exists())
            .map(|f| config::File::from(f.as_path()))
            .collect::<Vec<config::File<_, _>>>(),
        )
        .build()
        .map_err(|e| e.to_string())
}

fn ext_map_from_config<'a>(
    conf: &'a HashMap<String, HashMap<String, String>>,
) -> HashMap<String, Template> {
    let mut extmap = HashMap::new();
    for (k, v) in conf {
        println!(
            "{}: {} ({}) ⇒ {}",
            "Rule".blue().bold(),
            k,
            v["extensions"],
            v["command"]
        );
        for ext in v["extensions"].split(" ") {
            extmap.insert(ext.to_string(), Template::new(&v["command"]));
        }
    }
    extmap
}

fn render_command(
    cmd: &Option<Template>,
    conf_map: &HashMap<String, Template>,
    map: HashMap<&str, String>,
) -> String {
    if let Some(templ) = cmd {
        return templ.render_nofail(&map);
    }
    if let Some(templ) = conf_map.get(&(map["ext"].clone())) {
        return templ.render_nofail(&map);
    }
    return String::from("");
}

fn main() {
    let args = Cli::parse();
    let conf: HashMap<String, HashMap<String, String>> =
        get_config(&args.config).unwrap().try_deserialize().unwrap();
    let conf_map = ext_map_from_config(&conf);
    let cwd = env::current_dir().unwrap();
    let cng_templ = if args.template.len() > 0 {
        Some(Template::new(args.template))
    } else {
        None
    };
    let cmd_templ = if args.command.len() > 0 {
        Some(Template::new(args.command.join(" ")))
    } else {
        None
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::RecommendedWatcher::new(tx, notify::Config::default()).unwrap();
    let duration = Duration::from_millis(args.duration);
    let mut event_times = HashMap::<PathBuf, Instant>::new();

    let rm = if args.recursive {
        notify::RecursiveMode::Recursive
    } else {
        notify::RecursiveMode::NonRecursive
    };
    print!("{}: ", "Watching".bold().yellow());
    for path in args.watch {
        match watcher.watch(path.as_ref(), rm) {
            Ok(_) => (),
            Err(e) => {
                println!("\n{}: {}", "Error".bold().red(), e.to_string());
                return;
            }
        };
        print!("{:?} ", path);
    }
    println!("");

    for res in rx {
        match res {
            Ok(event) => match event.kind {
                notify::EventKind::Modify(notify::event::ModifyKind::Data(_)) => {
                    // this here ignores any consequtive events withing args.duration
                    // I think I should only act after the consequitive events are done...
                    // Need to figure that out later
                    if let Some(dur) = event_times.get(&event.paths[0]) {
                        if dur.elapsed() < duration {
                            continue;
                        }
                    }
                    event_times.insert(event.paths[0].clone(), Instant::now());
                    let mut map = template_vars(&event.paths[0], &cwd);
                    map.insert("event", format!("{:?}", event));
                    if let Some(templ) = &cng_templ {
                        println!(
                            "{}: {}",
                            "Changed".bold().green(),
                            templ.render_nofail(&map)
                        );
                    }
                    let cmd = render_command(&cmd_templ, &conf_map, map);
                    println!("{}: {}", "Run".bold().red(), cmd);
                    if args.r#async {
                        thread::spawn(|| {
                            Exec::shell(cmd).join().unwrap();
                        });
                    } else {
                        Exec::shell(cmd).join().unwrap();
                    }
                }
                _ => (),
            },
            Err(error) => {
                println!("Error {:?}", error)
            }
        }
    }
}
