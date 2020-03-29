extern crate i3ipc;
use i3ipc::I3Connection;
use i3ipc::reply::NodeType;
use i3ipc::reply::Node;
use std::process::{Command, Stdio};
use std::io::Write;

extern crate clap;
use clap::{App, AppSettings, SubCommand, ArgMatches, Arg};

use std::path::Path;
use std::fs::File;
use std::io::prelude::*;
use std::env::set_current_dir;

#[macro_use] extern crate failure;
use failure::Error;
use failure::err_msg;

extern crate yaml_rust;
use yaml_rust::{YamlLoader, Yaml};

extern crate shellexpand;
use shellexpand::tilde;

#[derive(Debug, Fail)]
enum WorkspaceExecError {
    #[fail(display = "No arguments supplied: {}", name)]
    NoArgumentError {
	name: String,
    }
}

fn main() {
    let matches = App::new("sway-action").version("v0.1.0")
					 .author("Rouven Czerwinski <rouven@czerwinskis.de>")
					 .about("Provides selections of sway $things via rofi")
					 .setting(AppSettings::ArgRequiredElseHelp)
					 .setting(AppSettings::TrailingVarArg)
					 .subcommand(SubCommand::with_name("focus-container")
						     .about("Focus window by name using rofi"))
					 .subcommand(SubCommand::with_name("steal-container")
						     .about("Steal window into current workspace"))
					 .subcommand(SubCommand::with_name("focus-workspace")
						     .about("Focus workspace by name using rofi"))
					 .subcommand(SubCommand::with_name("move-to-workspace")
						     .about("Move Currently focues container to workspace"))
					 .subcommand(SubCommand::with_name("move-workspace-to-output")
						     .about("Move current workspace to output by name"))
					 .subcommand(SubCommand::with_name("quick-window")
						     .about("Show quick-window"))
					 .subcommand(SubCommand::with_name("workspace-exec")
						     .about("execute command in workspace")
						     .arg(Arg::with_name("args").multiple(true)))
					 .get_matches();

    // establish a connection to i3 over a unix socket
    let err = || -> Result<(), Error> {
	let mut connection = I3Connection::connect()?;

	match matches.subcommand_name() {
    	    Some("focus-container") => Ok(focus_container_by_id(&mut connection)),
    	    Some("steal-container") => Ok(steal_container_by_id(&mut connection)),
    	    Some("focus-workspace") => Ok(focus_workspace_by_name(&mut connection)),
    	    Some("move-to-workspace") => Ok(move_to_workspace_by_name(&mut connection)),
    	    Some("move-workspace-to-output") => Ok(move_workspace_to_output(&mut connection)),
    	    Some("workspace-exec") => workspace_exec(&mut connection, &matches),
    	    Some("quick-window") => quick_window(&mut connection),
    	    _ => Ok({}),
	}
    }();

    if let Err(e) = err {
	println!("{}", err_msg(e));
	std::process::exit(1);
    }
    // request and print the i3 version
}

fn quick_window(conn: &mut I3Connection) -> Result<(), Error> {
    let nodes = conn.get_tree().expect("Could not get tree");
    match search_tree_for_mark(&nodes, "quick") {
	false => { Command::new("alacritty").arg("--class")
		   .arg("quick_scratchpad")
		   .arg("-e")
		   .arg("tmux").arg("new-session").arg("-A").arg("-s").arg("quick")
		   .spawn().expect("Could not start tmux"); }
	true => { Command::new("swaymsg").arg("[con_mark=\"quick\"]")
		  .arg("scratchpad")
		  .arg("show")
		  .spawn().expect("Could not run swaymsg"); }
    }
    Ok(())
}
fn workspace_exec(mut conn: &mut I3Connection, matches: &ArgMatches) -> Result<(), Error> {
    let matches = matches.subcommand_matches("workspace-exec").unwrap();
    let config = matches.value_of("config").unwrap_or("~/.config/sway-action/mapping.yaml");
    let config = tilde(config).to_string();
    let config_path = Path::new(&config);
    change_dir_from_mapping(&config_path, &mut conn)?;
    let args = matches.values_of("args")
		      .ok_or(WorkspaceExecError::NoArgumentError{ name: "No arguments found in input".to_string() })?;
    let mut args: std::vec::Vec<String> = args.collect::<Vec<_>>().into_iter().map(|s| s.to_owned()).collect();
    let binary = args.remove(0);
    std::process::Command::new(binary).args(&args).spawn()?;
    Ok(())
}

fn focus_container_by_id(mut conn: &mut I3Connection) {
    let containers = get_containers(&mut conn);

    let id = rofi_get_selection_id(&containers);
    conn.run_command(&format!("[con_id={}] focus", id)).expect("Can't change focus");
}

fn steal_container_by_id(mut conn: &mut I3Connection) {
    let windows = get_containers(&mut conn);

    let id = rofi_get_selection_id(&windows);
    conn.run_command(&format!("[con_id={}] move to workspace current", id)).expect(&format!("Can't focus window {}", id));
}

fn focus_workspace_by_name(mut conn: &mut I3Connection) {
    let work_names = get_workspaces(&mut conn);

    let space = rofi_get_selection(&work_names);
    conn.run_command(&format!("workspace {}", space)).expect(&format!("Can't focus workspace {}", space));
}

fn move_to_workspace_by_name(mut conn: &mut I3Connection) {
    let work_names = get_workspaces(&mut conn);

    let space = rofi_get_selection(&work_names);
    conn.run_command(&format!("move window to workspace {}", space)).expect(&format!("Can't focus workspace {}", space));
}

fn move_workspace_to_output(mut conn: &mut I3Connection) {
    let outputs = get_outputs(&mut conn);
    let output = rofi_get_selection_id(&outputs);
    conn.run_command(&format!("move workspace to output {}", output))
	.expect(&format!("Can't send to output {}", output));
}


fn get_outputs(conn: &mut I3Connection) -> Vec<String> {
    let outputs = conn.get_outputs().expect("Could not get workspaces");
    println!("{:?}", outputs.outputs);
    outputs.outputs.iter().filter(|x| x.active)
			  .map(|x| format!("{}: {} {} {}", x.name, x.make, x.model, x.serial).to_string())
			  .collect::<Vec<String>>()
}

fn get_workspaces(conn: &mut I3Connection) -> Vec<String> {
    let workspaces = conn.get_workspaces().expect("Could not get workspaces");
    workspaces.workspaces.iter().map(|x| x.name.to_string()).collect::<Vec<String>>()
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

fn rofi_get_selection_id(input: &Vec<String>) -> String {

    let rofi_out = rofi_run(&input);
    rofi_out.split(":").next().expect("Can't split out id").to_string()
}

fn rofi_get_selection(input: &Vec<String>) -> String {
    rofi_run(&input)
}

fn rofi_run(input: &Vec<String>) -> String {
    let mut child = Command::new("rofi").arg("-dmenu")
						 .arg("-i")
						 .stdin(Stdio::piped())
						 .stdout(Stdio::piped())
						 .spawn()
						 .expect("Can't open rofi");
    {
	let stdin = child.stdin.as_mut().expect("failed to get stdin");
	stdin.write_all(input.join("\n").as_bytes()).expect("failed to write to rofi");
    }
    let output = child
	.wait_with_output()
	.expect("failed to wait on child");
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
	    	    return true
		}
	    }
	}
	result = search_tree_for_mark(iter_node, search_mark);
	if result {
	    return true
	}
    }
    result
}

fn open_parse_config(path: &Path) -> Result<std::vec::Vec<Yaml>, Error> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let yaml = YamlLoader::load_from_str(&contents)?;
    Ok(yaml)
}

fn get_active_workspace(conn: &mut I3Connection) -> Result<String, Error> {
    let workspaces = conn.get_workspaces()?;
    Ok(workspaces.workspaces.iter().filter(|x| x.focused).map(|x| x.name.to_string()).collect())
}

fn change_dir_from_mapping(config: &Path, mut conn: &mut I3Connection) -> Result<bool, Error> {
    let workspace = get_active_workspace(&mut conn)?;
    let mapping = open_parse_config(&config);
    if let Err(e) = mapping {
	println!("Config Error: {}", err_msg(e));
	return Ok(false)
    }
    let mapping = mapping.unwrap();
    let dir = match mapping[0]["mapping"][&workspace[..]].clone().into_string() {
	Some(s) => tilde(&s).to_string(),
	None => tilde("~").to_string(),
    };
    match set_current_dir(dir) {
	Ok(_) => Ok(true),
	Err(e) => {
	    println!("Could not set dir: {}", err_msg(e));
	    std::process::exit(1);
	}
    }
}
