use blockfield_core::core::*;
use serde_json::{json, Value};
use std::{
    io::{BufRead, Read},
    path::PathBuf,
    time::Duration,
};
fn content_session(directory: PathBuf) {
    let service = Service::new(directory).unwrap();
    service.set_sink(std::sync::Arc::new(|event, payload| {
        println!("{}", json!({"event":event,"payload":payload}));
    }));
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(input) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let service = service.clone();
        std::thread::spawn(move || {
            let args = &input["args"];
            let result = match input["command"].as_str().unwrap_or("") {
                "snapshot" => service.snapshot(),
                "save_settings" => service.save(args["patch"].clone()),
                "list_mods" => service.mods(),
                "find_java" => service.javas(false).map(|v| json!(v)),
                "modrinth_search" => service.modrinth_search(
                    args["query"].as_str().unwrap_or(""),
                    args["kind"].as_str().unwrap_or("mod"),
                    args["offset"].as_u64().unwrap_or(0) as u32,
                    args["sort"].as_str().unwrap_or("relevance"),
                ),
                "modrinth_versions" => service.modrinth_versions(
                    args["projectId"].as_str().unwrap_or(""),
                    args["kind"].as_str().unwrap_or("mod"),
                ),
                "modrinth_prepare" => service.modrinth_prepare(
                    args["projectId"].as_str().unwrap_or(""),
                    args["versionId"].as_str().unwrap_or(""),
                    args["kind"].as_str().unwrap_or("mod"),
                ),
                "modrinth_install" => {
                    service.modrinth_install(args["planId"].as_str().unwrap_or(""))
                }
                "modrinth_cancel" => {
                    service.modrinth.cancel();
                    Ok(Value::Null)
                }
                "auth_cancel" => {
                    service.auth.cancel();
                    Ok(Value::Null)
                }
                _ => Err("Unsupported development command".into()),
            };
            match result {
                Ok(value) => println!("{}", json!({"id":input["id"],"ok":true,"value":value})),
                Err(error) => println!("{}", json!({"id":input["id"],"ok":false,"error":error})),
            };
        });
    }
}
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--content-session") {
        content_session(PathBuf::from(
            std::env::args().nth(2).expect("data directory"),
        ));
        return;
    }
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    let result = (|| {
        let service = Service::new(PathBuf::from(input["dataDir"].as_str().unwrap()))?;
        match input["operation"].as_str().unwrap_or("snapshot") {
            "modrinth" => {
                if input["settings"].is_object() {
                    service.save(input["settings"].clone())?;
                }
                let kind = input["kind"].as_str().unwrap_or("mod");
                match input["action"].as_str().unwrap_or("search") {
                    "search" => service.modrinth_search(
                        input["query"].as_str().unwrap_or(""),
                        kind,
                        0,
                        "relevance",
                    ),
                    "versions" => {
                        service.modrinth_versions(input["projectId"].as_str().unwrap(), kind)
                    }
                    "install" | "cancel" => {
                        if input["action"] == "cancel" {
                            let cancel = service.clone();
                            service.set_sink(std::sync::Arc::new(move |kind, value| {
                                if kind == "state" && value["progress"].is_object() {
                                    cancel.modrinth.cancel();
                                }
                            }));
                        }
                        let plan = service.modrinth_prepare(
                            input["projectId"].as_str().unwrap(),
                            input["versionId"].as_str().unwrap(),
                            kind,
                        )?;
                        let result = service.modrinth_install(plan["planId"].as_str().unwrap());
                        if input["action"] == "cancel" {
                            Ok(
                                json!({"cancelled":result==Err("下载已取消。".into()),"plan":plan,"mods":service.mods()?}),
                            )
                        } else {
                            Ok(json!({"plan":plan,"result":result?,"mods":service.mods()?}))
                        }
                    }
                    _ => Err("Unknown Modrinth action".into()),
                }
            }
            "snapshot" => service.snapshot(),
            "authProbe" => {
                let challenge = service.auth.start()?;
                let summary = json!({"deviceAuthorization":true,"verificationUri":challenge["verificationUri"],"interval":challenge["interval"],"expiresAt":challenge["expiresAt"],"configured":service.auth.summary()["configured"]});
                service.auth.cancel();
                Ok(summary)
            }
            "save" => service.save(input["settings"].clone()),
            "addDirectory" => service.add_directory(input["selected"].as_str().unwrap()),
            "plan" => {
                if input["settings"].is_object() {
                    service.save(input["settings"].clone())?;
                }
                serde_json::to_value(service.build_plan()?).map_err(|e| e.to_string())
            }
            "check" => {
                if input["settings"].is_object() {
                    service.save(input["settings"].clone())?;
                }
                service.check()
            }
            "describe" => Ok(describe_version(&input["version"])),
            "uuid" => Ok(json!(offline_uuid(input["username"].as_str().unwrap()))),
            "launch" => {
                if input["settings"].is_object() {
                    service.save(input["settings"].clone())?;
                }
                let result = service.launch()?;
                let limit = input["durationSeconds"].as_u64().unwrap_or(150);
                for _ in 0..limit * 10 {
                    if !service.has_child() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if service.has_child() {
                    service.stop()?;
                    std::thread::sleep(Duration::from_millis(500));
                }
                Ok(json!({"launch":result,"snapshot":service.snapshot()?}))
            }
            _ => Err("Unknown operation".into()),
        }
    })();
    match result {
        Ok(result) => println!("{}", json!({"ok":true,"value":result})),
        Err(error) => {
            println!("{}", json!({"ok":false,"error":error}));
            std::process::exit(1);
        }
    }
}
