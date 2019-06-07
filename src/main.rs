extern crate i3ipc;
use i3ipc::I3Connection;
use i3ipc::reply::NodeType;
use i3ipc::reply::Node;
use std::process::{Command, Stdio};
use std::io::Write;
extern crate clap;
use clap::{App, AppSettings, SubCommand};

fn main() {
    let matches = App::new("sway-action").version("v0.1.0")
                                         .author("Rouven Czerwinski <rouven@czerwinskis.de>")
                                         .about("Provides selections of sway $things via rofi")
                                         .setting(AppSettings::ArgRequiredElseHelp)
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
                                         .get_matches();

    // establish a connection to i3 over a unix socket
    let mut connection = I3Connection::connect().expect("Could not open I3 Connection");

    match matches.subcommand_name() {
        Some("focus-container") => focus_container_by_id(&mut connection),
        Some("steal-container") => steal_container_by_id(&mut connection),
        Some("focus-workspace") => focus_workspace_by_name(&mut connection),
        Some("move-to-workspace") => move_to_workspace_by_name(&mut connection),
        Some("move-workspace-to-output") => move_workspace_to_output(&mut connection),
        _ => {},
    }
    // request and print the i3 version
}

fn focus_container_by_id(mut conn: &mut I3Connection) {
    let containers = get_containers(&mut conn);

    let id = rofi_get_selection_id(&containers);
    conn.run_command(&format!("[con_id={}] focus", id)).expect(&format!("Can't focus container {}", id));
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
    let mut child = Command::new("/usr/bin/rofi").arg("-dmenu")
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
