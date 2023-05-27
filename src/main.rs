use clap::Parser;
use colored::Colorize;
use config;
use humantime::parse_duration;
use new_string_template::template::Template;
use notify_debouncer_mini::{new_debouncer, notify, DebouncedEventKind};
use std::io::{BufRead, BufReader};
use std::time::Duration;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use std::{env, thread};
use subprocess::Exec;

#[derive(Parser)]
struct Cli {
    /// Config file, ignored if command is given directly
    ///
    /// If none given it'll search the following paths:
    ///
    /// - "/etc/onchange.toml"
    /// - "~/.config/onchange.toml"
    /// - ".onchange.toml"
    /// The later will overwrite the former if same config is present.
    #[arg(short, long)]
    config: Option<String>,
    /// Debouncer duration (treat multiple events within this as one)
    #[arg(short='D', long, default_value = "500ms", value_parser=parse_duration)]
    duration: Duration,
    /// Delay duration before execution of the command
    #[arg(short, long, default_value = "50us", value_parser=parse_duration)]
    delay: Duration,
    /// Watch in Recursive Mode
    #[arg(short, long, action)]
    recursive: bool,
    /// Render the command but do not run it
    #[arg(short = 'R', long, action)]
    render_only: bool,
    /// Run commands on Async
    #[arg(short, long, action)]
    r#async: bool,
    /// Ignore pattern, use unix shell style glob pattern
    #[arg(short, long, default_value = "")]
    ignore: Vec<glob::Pattern>,
    /// Template to get more informations on changed file
    ///
    /// You can use commands with similar template to command that'll
    /// output a list of key:val lines in stdout, that this program
    /// will use as extra variables to populate your command template,
    /// and change template.
    #[arg(short, long)]
    variables_command: Option<String>,
    /// Template to show informations on file change detection
    #[arg(short, long, default_value = "{path}")]
    template: String,
    /// Trial run
    #[arg(short = 'T', long, action, conflicts_with = "recursive")]
    trial_run: bool,
    /// List paths to watch, any number of file is fine
    #[arg(num_args(1..), required(true))]
    watch: Vec<PathBuf>,
    /// Command to run, use single quotes to skip the template braces
    /// properly
    #[arg(num_args(0..), last(true))]
    command: Vec<String>,
}

fn template_vars(
    path: &PathBuf,
    pwd: &PathBuf,
    var_cmd: &Option<String>,
    conf_map: &HashMap<String, (Option<Template>, Option<Template>)>,
) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    map.insert(
        "name".to_string(),
        path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    );
    map.insert(
        "ext".to_string(),
        path.extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    );
    map.insert(
        "name.ext".to_string(),
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    );
    map.insert("pwd".to_string(), pwd.to_string_lossy().to_string());
    map.insert("path".to_string(), path.to_string_lossy().to_string());
    map.insert(
        "rpath".to_string(),
        pathdiff::diff_paths(path, pwd)
            .unwrap()
            .to_string_lossy()
            .to_string(),
    );
    let parent = path.parent().unwrap_or(Path::new("/"));
    map.insert("dir".to_string(), parent.to_string_lossy().to_string());
    map.insert(
        "rdir".to_string(),
        pathdiff::diff_paths(parent, pwd)
            .unwrap()
            .to_string_lossy()
            .to_string(),
    );
    map.insert(
        "rname".to_string(),
        format!(
            "{}{}{}",
            map["rdir"],
            std::path::MAIN_SEPARATOR,
            map["name.ext"]
        ),
    );

    // populate it with more variables from the command. If given
    // from CLI use it, otherwise use the one from config.
    let var_cmd = match var_cmd {
        Some(cmd) => Some(Template::new(cmd.clone())),
        None => match conf_map.get(&map["ext"]) {
            Some((_, Some(templ))) => Some(templ.clone()),
            _ => None,
        },
    };

    if let Some(cmd_t) = var_cmd {
        let cmd = cmd_t.render_string(&map).unwrap();
        BufReader::new(Exec::shell(cmd).stream_stdout().unwrap())
            .lines()
            .for_each(|s| {
                if let Some((k, v)) = s.unwrap().split_once(":") {
                    map.insert(k.trim().to_string(), v.trim().to_string());
                }
            });
    }
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
    verbose: bool,
) -> HashMap<String, (Option<Template>, Option<Template>)> {
    let mut extmap = HashMap::new();
    for (k, v) in conf {
        if verbose {
            print!("{}: {} ({})", "Rule".blue().bold(), k, v["extensions"],);
            if let Some(cmd) = v.get("command") {
                print!(" ⇒ {}", cmd);
            }
            println!("");
        }
        for ext in v["extensions"].split(" ") {
            extmap.insert(
                ext.to_string(),
                (
                    v.get("command").map(Template::new),
                    v.get("extra_variables").map(Template::new),
                ),
            );
        }
    }
    extmap
}

fn on_change(args: &Cli, path: &PathBuf, cmd: String, cng: Option<String>) {
    {
        if args.ignore.iter().any(|p| p.matches_path(&path)) {
            return;
        }
        if let Some(templ) = &cng {
            println!("{}: {}", "Changed".bold().green(), templ);
        }

        if cmd.is_empty() {
            return;
        }
        println!("{}: {}", "Run".bold().red(), cmd);
        if args.render_only {
            return;
        }
        let del = args.delay;
        if args.r#async {
            thread::spawn(move || {
                thread::sleep(del);
                Exec::shell(cmd).join().unwrap();
            });
        } else {
            thread::sleep(args.delay);
            Exec::shell(cmd).join().unwrap();
        }
    }
}

fn render_command(
    cmd: &Option<Template>,
    conf_map: &HashMap<String, (Option<Template>, Option<Template>)>,
    map: &HashMap<String, String>,
) -> String {
    if let Some(templ) = cmd {
        return templ.render_nofail_string(&map);
    }
    if let Some((Some(templ), _)) = conf_map.get(&(map["ext"].clone())) {
        return templ.render_nofail_string(&map);
    }
    return String::from("");
}

fn main() {
    let args = Cli::parse();
    let conf_map = if args.command.len() > 0 && args.variables_command.is_some() {
        HashMap::new()
    } else {
        let conf: HashMap<String, HashMap<String, String>> = match get_config(&args.config)
            .and_then(|c| c.try_deserialize().map_err(|e| e.to_string()))
        {
            Ok(conf) => conf,
            Err(e) => {
                println!("\n{}: {}", "Error".bold().red(), e);
                return;
            }
        };
        ext_map_from_config(&conf, args.command.len() == 0)
    };
    let cwd = env::current_dir().unwrap();
    let cng_templ = if args.template.len() > 0 {
        Some(Template::new(&args.template))
    } else {
        None
    };
    let cmd_templ = if args.command.len() > 0 {
        Some(Template::new(args.command.join(" ")))
    } else {
        None
    };

    if args.trial_run {
        for path in &args.watch {
            // TODO suport recursive flag

            // HACK TODO use proper methods to find absolute path, or
            // verify this is good enough
            let path = if path.is_relative() {
                cwd.join(path)
            } else {
                path.clone()
            };
            let map = template_vars(&path, &cwd, &args.variables_command, &conf_map);
            let cmd = render_command(&cmd_templ, &conf_map, &map);
            let cng = cng_templ.as_ref().map(|t| t.render_nofail_string(&map));
            on_change(&args, &path, cmd, cng)
        }
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();

    let mut debouncer = new_debouncer(args.duration, None, tx).unwrap();

    let rm = if args.recursive {
        notify::RecursiveMode::Recursive
    } else {
        notify::RecursiveMode::NonRecursive
    };
    let watcher = debouncer.watcher();
    print!("{}: ", "Watching".bold().yellow());
    for path in &args.watch {
        match watcher.watch(path.as_ref(), rm) {
            Ok(_) => print!("{:?} ", path),
            Err(e) => {
                println!("\n{}: {}", "Error".bold().red(), e.to_string());
                return;
            }
        };
    }
    println!("");

    for res in rx {
        match res {
            Ok(events) => events.iter().for_each(|event| match event.kind {
                DebouncedEventKind::Any => {
                    let mut map =
                        template_vars(&event.path, &cwd, &args.variables_command, &conf_map);
                    map.insert("event".to_string(), format!("{:?}", event));
                    let cmd = render_command(&cmd_templ, &conf_map, &map);
                    let cng = cng_templ.as_ref().map(|t| t.render_nofail_string(&map));
                    on_change(&args, &event.path, cmd, cng);
                }
                _ => (),
            }),
            Err(errors) => errors.iter().for_each(|e| println!("Error {:?}", e)),
        }
    }
}
