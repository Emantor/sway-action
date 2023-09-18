extern crate i3ipc;
use i3ipc::reply::Node;
use i3ipc::reply::NodeType;
use i3ipc::I3Connection;
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

extern crate clap;
use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};

use std::env::set_current_dir;
use std::path::Path;

#[macro_use]
extern crate failure;
use failure::Error;

extern crate shellexpand;
use shellexpand::tilde;

#[derive(Debug, Fail)]
enum WorkspaceExecError {
    #[fail(display = "No arguments supplied: {}", name)]
    NoArgumentError { name: String },
    #[fail(display = "Failed setting current workdir")]
    DirSetupError,
    #[fail(display = "Configuration wrong")]
    ConfigError,
}

struct ApplicationState<'a> {
    conn: &'a mut I3Connection,
    confdir: &'a Path,
}

fn main() -> Result<(), Error> {
    let matches = App::new("sway-action")
        .version("v0.1.7")
        .author("Rouven Czerwinski <rouven@czerwinskis.de>")
        .about("Provides selections of sway $things via bemenu")
        .setting(AppSettings::ArgRequiredElseHelp)
        .setting(AppSettings::TrailingVarArg)
        .arg(Arg::with_name("confdir").default_value("~/.config/sway-action/"))
        .subcommand(
            SubCommand::with_name("focus-container").about("Focus window by name using bemenu"),
        )
        .subcommand(
            SubCommand::with_name("steal-container").about("Steal window into current workspace"),
        )
        .subcommand(
            SubCommand::with_name("focus-workspace").about("Focus workspace by name using bemenu"),
        )
        .subcommand(
            SubCommand::with_name("move-to-workspace")
                .about("Move Currently focues container to workspace"),
        )
        .subcommand(
            SubCommand::with_name("move-workspace-to-output")
                .about("Move current workspace to output by name"),
        )
        .subcommand(
            SubCommand::with_name("quick-window")
                .about("Show quick-window")
                .arg(Arg::with_name("window")),
        )
        .subcommand(
            SubCommand::with_name("workspace-exec")
                .about("execute command in workspace")
                .arg(Arg::with_name("args").multiple(true)),
        )
        .get_matches();

    // establish a connection to i3 over a unix socket
        let config = tilde(matches.value_of("confdir").unwrap()).to_string();
        let mut state = ApplicationState {
            conn: &mut I3Connection::connect().unwrap(),
            confdir: &Path::new(&config),
        };

    match matches.subcommand_name() {
        Some("focus-container") => Ok(focus_container_by_id(&mut state)),
        Some("steal-container") => Ok(steal_container_by_id(&mut state)),
        Some("focus-workspace") => Ok(focus_workspace_by_name(&mut state)),
        Some("move-to-workspace") => Ok(move_to_workspace_by_name(&mut state)),
        Some("move-workspace-to-output") => Ok(move_workspace_to_output(&mut state)),
        Some("workspace-exec") => workspace_exec(&mut state, &matches),
        Some("quick-window") => quick_window(&mut state, &matches),
        _ => Ok({}),
    }
}

fn quick_window(state: &mut ApplicationState, matches: &ArgMatches) -> Result<(), Error> {
    let matches = matches.subcommand_matches("quick-window").unwrap();
    let nodes = state.conn.get_tree().expect("Could not get tree");
    let quick_path = state.confdir.join("quickmap");
    let window = matches.value_of("window").unwrap_or("quick");
    let map = std::fs::read_to_string(quick_path)?
        .lines()
        .map(|s| s.split(": "))
        .fold(HashMap::new(), |mut acc, x| {
            acc.insert(
                x.clone().next().unwrap().to_string(),
                x.clone().skip(1).next().unwrap().to_string(),
            );
            acc
        });

    let commands = map
        .get(&window[..])
        .ok_or(WorkspaceExecError::ConfigError)?
        .split(' ')
        .collect::<Vec<&str>>();

    match search_tree_for_mark(&nodes, window) {
        false => {
            Command::new(commands[0])
                .args(&commands[1..])
                .spawn()
                .expect("Could not start tmux");
        }
        true => {
            Command::new("swaymsg")
                .arg(format!("[con_mark=\"{}\"]", window))
                .arg("scratchpad")
                .arg("show")
                .stdout(Stdio::piped())
                .output()
                .expect("Could not run swaymsg");
        }
    }
    Ok(())
}
fn workspace_exec(state: &mut ApplicationState, matches: &ArgMatches) -> Result<(), Error> {
    let matches = matches.subcommand_matches("workspace-exec").unwrap();
    let mapping_path = state.confdir.join("mapping");
    match change_dir_from_mapping(&mapping_path, &mut state.conn) {
        Err(e) => {
            println!("Error: {}", e);
            println!("Continuing without changing directory");
        }
        Ok(_) => (),
    };
    let args = matches
        .values_of("args")
        .ok_or(WorkspaceExecError::NoArgumentError {
            name: "No arguments found in input".to_string(),
        })?;
    let mut args: std::vec::Vec<String> = args
        .collect::<Vec<_>>()
        .into_iter()
        .map(|s| s.to_owned())
        .collect();
    let binary = args.remove(0);
    std::process::Command::new(binary).args(&args).spawn()?;
    Ok(())
}

fn focus_container_by_id(state: &mut ApplicationState) {
    let containers = get_containers(&mut state.conn);

    let id = bemenu_get_selection_id(&containers);
    state
        .conn
        .run_command(&format!("[con_id={}] focus", id))
        .expect("Can't change focus");
}

fn steal_container_by_id(state: &mut ApplicationState) {
    let windows = get_containers(&mut state.conn);

    let id = bemenu_get_selection_id(&windows);
    state
        .conn
        .run_command(&format!("[con_id={}] move to workspace current", id))
        .expect(&format!("Can't focus window {}", id));
}

fn focus_workspace_by_name(state: &mut ApplicationState) {
    let work_names = get_workspaces(&mut state.conn);

    let space = bemenu_get_selection(&work_names);
    state
        .conn
        .run_command(&format!("workspace {}", space))
        .expect(&format!("Can't focus workspace {}", space));
}

fn move_to_workspace_by_name(state: &mut ApplicationState) {
    let work_names = get_workspaces(&mut state.conn);

    let space = bemenu_get_selection(&work_names);
    state
        .conn
        .run_command(&format!("move window to workspace {}", space))
        .expect(&format!("Can't focus workspace {}", space));
}

fn move_workspace_to_output(state: &mut ApplicationState) {
    let outputs = get_outputs(&mut state.conn);
    let output = bemenu_get_selection_id(&outputs);
    state
        .conn
        .run_command(&format!("move workspace to output {}", output))
        .expect(&format!("Can't send to output {}", output));
}

fn get_outputs(conn: &mut I3Connection) -> Vec<String> {
    let outputs = conn.get_outputs().expect("Could not get workspaces");
    println!("{:?}", outputs.outputs);
    outputs
        .outputs
        .iter()
        .filter(|x| x.active)
        .map(|x| format!("{}: {} {} {}", x.name, x.make, x.model, x.serial).to_string())
        .collect::<Vec<String>>()
}

fn get_workspaces(conn: &mut I3Connection) -> Vec<String> {
    let workspaces = conn.get_workspaces().expect("Could not get workspaces");
    workspaces
        .workspaces
        .iter()
        .map(|x| x.name.to_string())
        .collect::<Vec<String>>()
}

fn get_containers(conn: &mut I3Connection) -> Vec<String> {
    let nodes = conn.get_tree().expect("Could not get tree");
    get_all_by_type(&nodes, &NodeType::Con)
}

fn get_all_by_type(node: &Node, node_type: &NodeType) -> Vec<String> {
    let mut res: Vec<String> = Vec::new();
    for iter_node in node.nodes.iter() {
        if let Some(name) = &iter_node.name {
            if iter_node.nodetype == *node_type {
                res.push(format!("{}: {}", iter_node.id, name))
            }
        }
        res.extend(get_all_by_type(&iter_node, node_type));
    }
    res
}

fn bemenu_get_selection_id(input: &Vec<String>) -> String {
    let bemenu_out = bemenu_run(&input);
    bemenu_out
        .split(":")
        .next()
        .expect("Can't split out id")
        .to_string()
}

fn bemenu_get_selection(input: &Vec<String>) -> String {
    bemenu_run(&input)
}

fn bemenu_run(input: &Vec<String>) -> String {
    let mut child = Command::new("bemenu")
        .arg("--fn")
        .arg("Monospace 16")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Can't open bemenu");
    {
        let stdin = child.stdin.as_mut().expect("failed to get stdin");
        stdin
            .write_all(input.join("\n").as_bytes())
            .expect("failed to write to bemenu");
    }
    let output = child.wait_with_output().expect("failed to wait on child");
    String::from_utf8(output.stdout).expect("Can't read output")
}

fn search_tree_for_mark(node: &Node, search_mark: &str) -> bool {
    let mut result = false;
    let mut combined = node.nodes.clone();
    combined.append(&mut node.floating_nodes.clone());
    for iter_node in combined.iter() {
        if let Some(marks) = &iter_node.marks {
            for mark in marks.iter() {
                let res = search_mark == mark;
                if res {
                    return true;
                }
            }
        }
        result = search_tree_for_mark(iter_node, search_mark);
        if result {
            return true;
        }
    }
    result
}

fn get_active_workspace(conn: &mut I3Connection) -> Result<String, Error> {
    let workspaces = conn.get_workspaces()?;
    Ok(workspaces
        .workspaces
        .iter()
        .filter(|x| x.focused)
        .map(|x| x.name.to_string())
        .collect())
}

fn change_dir_from_mapping(config: &Path, mut conn: &mut I3Connection) -> Result<(), Error> {
    let workspace = get_active_workspace(&mut conn)?;
    let map = std::fs::read_to_string(config)?
        .lines()
        .map(|s| s.split(": "))
        .fold(HashMap::new(), |mut acc, x| {
            acc.insert(
                x.clone().next().unwrap().to_string(),
                x.clone().skip(1).next().unwrap().to_string(),
            );
            acc
        });

    let dir = match map.get(&workspace[..]) {
        Some(s) => tilde(&s).to_string(),
        None => tilde("~").to_string(),
    };

    let path = Path::new(&dir);

    if !path.exists() {
        return Ok(());
    }

    match set_current_dir(dir) {
        Ok(_) => Ok(()),
        Err(_) => Err(WorkspaceExecError::DirSetupError)?,
    }
}
