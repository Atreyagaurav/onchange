use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("Argument 1 needs to be a path");
    println!("watching {}", path);
    let (tx, rx) = std::sync::mpsc::channel();

    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let mut watcher = RecommendedWatcher::new(tx, Config::default()).unwrap();

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(path.as_ref(), RecursiveMode::Recursive).unwrap();

    for res in rx {
        match res {
            Ok(event) => println!("changed: {:?}", event),
            Err(e) => println!("watch error: {:?}", e),
        }
    }
}
