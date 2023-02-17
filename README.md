# Introduction
OnChange is a CLI utility that can run commands when it detects changes on the files you ask it to watch. 

The syntax is: `onchange file1 file2... -- command`. You have to separate the command with `--` or you can have no command if you only want to see the changes. There are other flags like the template for change message, and flags for async execution of the command. 

The async flag will help you if you don't want to miss on other changes when the command is executing, each command will be executed in a thread with this flag.

If you want more functionality there is a tool with more options than this one: [watchexec](https://github.com/watchexec/watchexec).

# Demo

![Screenshot](images/screenshot.png)

Here in this image, the pdf is shown in green box (right), which is being compiled anytime the `.tex` file or `graph.pdf` file is changed (blue box). And the `graph.pdf` file is generated every time `graph.gv` file is changed (yellow box). So when you change the graviz file (red box), the `graph.pdf` is updated and then the final pdf. Hence, you can use it when you need to review changes without having to re-compile stuffs.

[Video Demo](https://youtu.be/PbEqUU-tBXQ)

# Usage

## Command Template
You can put a simple command after `--` that'll run the same, or you can use some variables based on the file the changes were detected on.

The templates for the file change detect, and the command can have few variables. Pass the template with these variables inside curly braces `{}`. Remember to escape the curly braces itself.

| Variable | Value                                             |
|----------|---------------------------------------------------|
| path     | full path of the changed file                     |
| rpath    | relative path of the changed file wrt PWD         |
| dir      | directory (parent) of the changed file (absolute) |
| rdir     | directory (parent) of the changed file (relative) |
| name     | filename of the changed file                      |
| ext      | extension of the changed file (excludes `.`)      |
| name.ext | name and extension of the changed file            |

For example: you can do `onchange --recursive . --template '{path}'` to watch any file change in a working directory. Similarly, you can use other variables to be creative with the commands.

## config file
You can use config files to determine the default actions for some file extensions. If you give commands then the config file will be ignored.

The config will be read from these locations:
- "/etc/onchange.toml"
- "$HOME/.config/onchange.toml"
- ".onchange.toml"
The later will overwrite the former if same config is present. And if you provide a file with `--config` flag, then none of these will be read and only the config from the fill will be used.

The format of the config file should be something like:

    [latex]
    extensions="tex"
    command="latexmk -pdf {name.ext}"

Here the rule will be in `[]` and then the space separated list of extensions to apply this rule to, and then command template to run. You can put a rule with empty command if you want to make "ignore rule" (though it'll be detected and shown). Here, with this config, any change in `.tex` file will run `latexmk` command on that file to generate a pdf.

# Help

`onchange --help` will give you the help menu with usage details.


    Usage: onchange [OPTIONS] <WATCH>... [-- [COMMAND]...]
    
    Arguments:
      <WATCH>...    List paths to watch
      [COMMAND]...  Command to run
    
    Options:
      -d, --duration <DURATION>  Scan duration in miliseconds [default: 500]
      -r, --recursive            Watch in Recursive Mode
      -a, --async                Run commands on Async
      -t, --template <TEMPLATE>  List paths to watch [default: "Change Detected: {path}"]
      -h, --help                 Print help


# Inspiration
The need to have something run on file change is everywhere, and I had been using a shell script for latex files to compile when the file changed. But since I might need it for lots of other stuffs too, like the graphviz example here. I thought of making a program to specialize in it. 
