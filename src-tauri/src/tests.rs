use crate::core::*;
use serde_json::json;
use std::{fs, path::PathBuf};
fn temp(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "blockfield-rust-{}-{}-{}",
        name,
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
#[test]
fn standard_offline_identity() {
    assert_eq!(offline_uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    assert_ne!(offline_uuid("Explorer"), offline_uuid("explorer"));
}
#[test]
fn rule_order_and_features() {
    let rules = json!([{"action":"allow","os":{"name":"windows"}},{"action":"disallow","features":{"is_demo_user":true}}]);
    assert!(allowed(&rules, &json!({})));
    assert!(!allowed(&rules, &json!({"is_demo_user":true})));
    assert!(!allowed(
        &json!([{"action":"allow","os":{"arch":"x86"}}]),
        &json!({})
    ));
}
#[test]
fn traversal_is_rejected() {
    let root = temp("path");
    assert!(inside(&root, "../escape").is_err());
    assert!(inside(&root, "C:/escape").is_err());
    assert!(inside(&root, "libraries/a.jar").unwrap().starts_with(&root));
}
#[test]
fn arguments_keep_spaces_and_unknowns_fail() {
    let args=expand_arguments(&json!(["--gameDir","${game_directory}",{"rules":[{"action":"allow","features":{"has_custom_resolution":true}}],"value":["--width","${resolution_width}"]}]),&json!({"game_directory":"D:\\中文 空格\\游戏","resolution_width":1280}),&json!({"has_custom_resolution":true})).unwrap();
    assert_eq!(
        args,
        vec!["--gameDir", "D:\\中文 空格\\游戏", "--width", "1280"]
    );
    assert!(expand_arguments(&json!(["${unknown}"]), &json!({}), &json!({})).is_err());
}
#[test]
fn native_architecture_selection() {
    assert!(native_arch("org.lwjgl:lwjgl:3:natives-windows"));
    assert!(!native_arch("org.lwjgl:lwjgl:3:natives-windows-arm64"));
    assert!(!native_arch("org.lwjgl:lwjgl:3:natives-windows-x86"));
}
#[test]
fn loader_java_metadata() {
    assert_eq!(
        describe_version(
            &json!({"id":"Custom","minecraftId":"1.21.1","libraries":[{"name":"net.neoforged:neoforge:21.1.200"}]})
        )["javaRequired"],
        21
    );
    assert_eq!(
        describe_version(
            &json!({"id":"Custom","libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.4.0"}]})
        )["minecraft"],
        "1.20.1"
    );
    assert_eq!(
        describe_version(&json!({"id":"future","javaVersion":{"majorVersion":25}}))["javaRequired"],
        25
    );
}
#[test]
fn fresh_settings_no_fixed_pack() {
    let service = Service::new(temp("blank")).unwrap();
    assert_eq!(service.settings()["gameRoot"], "");
    assert_eq!(service.settings()["javaMode"], "auto");
    assert!(service.build_plan().is_err());
    assert_eq!(service.mods().unwrap(), json!([]));
}
#[test]
fn profiles_are_separate_and_persistent() {
    let root = temp("profiles");
    let game = root.join(".minecraft");
    for id in ["A", "B"] {
        let dir = game.join("versions").join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(format!("{id}.json")),
            json!({"id":id,"mainClass":"Main","libraries":[]}).to_string(),
        )
        .unwrap();
    }
    let data = root.join("data");
    let service = Service::new(data.clone()).unwrap();
    service.add_directory(game.to_str().unwrap()).unwrap();
    service
        .save(json!({"versionId":"A","memoryMB":2048,"extraJvmArgs":"-Dpack=A"}))
        .unwrap();
    service.save(json!({"versionId":"B"})).unwrap();
    assert_eq!(service.settings()["extraJvmArgs"], "");
    service.save(json!({"memoryMB":3072})).unwrap();
    service.save(json!({"versionId":"A"})).unwrap();
    assert_eq!(service.settings()["memoryMB"], 2048);
    assert_eq!(
        Service::new(data).unwrap().settings()["extraJvmArgs"],
        "-Dpack=A"
    );
}
#[test]
fn inheritance_and_shared_layout() {
    let root = temp("inheritance");
    for (id, value) in [
        (
            "base",
            json!({"id":"1.21.1","mainClass":"Main","libraries":[{"name":"group:lib:1"}],"arguments":{"game":["--version","${version_name}"]}}),
        ),
        (
            "forge",
            json!({"inheritsFrom":"base","mainClass":"Bootstrap","libraries":[{"name":"group:lib:2"}],"arguments":{"game":["--launchTarget","forgeclient"]}}),
        ),
    ] {
        let dir = root.join("versions").join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{id}.json")), value.to_string()).unwrap();
    }
    let v = resolve_version(&root, "forge").unwrap();
    assert_eq!(v["jarId"], "base");
    assert_eq!(v["libraries"][0]["name"], "group:lib:2");
    assert_eq!(
        game_directory(&root, "forge", &json!("auto")).unwrap(),
        root
    );
}

fn empty_plan(root: &std::path::Path) -> Plan {
    Plan {
        entry_index: 0,
        sensitive_token: None,
        config: json!({"gameRoot":root}),
        version: json!({"assetIndex":{"id":"legacy"}}),
        game_dir: root.join("versions/legacy"),
        native_dir: root.join("natives"),
        native_jars: vec![],
        classpath: vec![],
        files: vec![FileItem {
            file: root.join("assets/indexes/legacy.json"),
            sha1: None,
            url: None,
            label: "资源索引".into(),
            reason: None,
        }],
        args: vec![],
    }
}
#[test]
fn late_process_logs_still_hide_tokens_after_logout() {
    let service = Service::new(temp("late-token-log")).unwrap();
    let bytes = b"late log with dummy-private-token\n".to_vec();
    capture(
        std::io::Cursor::new(bytes),
        service.clone(),
        "info",
        Some("dummy-private-token".into()),
    );
    let logs = service.snapshot().unwrap()["logs"].to_string();
    assert!(logs.contains("[已隐藏]"));
    assert!(!logs.contains("dummy-private-token"));
}
#[test]
fn online_tokens_stay_out_of_long_argument_files() {
    let root = temp("online-arguments");
    let service = Service::new(root.join("data")).unwrap();
    let mut plan = empty_plan(&root);
    let secret = "dummy-private-minecraft-token";
    plan.args = vec![
        "-cp".into(),
        "library.jar;".repeat(2500),
        "Main".into(),
        "--accessToken".into(),
        secret.into(),
        "--session".into(),
        format!("token:{secret}:0123456789abcdef0123456789abcdef"),
        "--username".into(),
        "GenuinePlayer".into(),
    ];
    plan.sensitive_token = Some(secret.into());
    plan.entry_index = 2;
    let (args, env) = service
        .invocation(&plan, &json!({"major":22,"encoding":"UTF-8"}))
        .unwrap();
    assert!(env.is_none());
    assert!(args.iter().any(|a| a == secret));
    for arg in args.iter().filter(|a| a.starts_with('@')) {
        assert!(!fs::read_to_string(arg.trim_start_matches('@'))
            .unwrap()
            .contains(secret));
    }
}
#[test]
fn actual_java_expands_secret_free_argument_files_in_order() {
    let Ok(java) = std::env::var("BLOCKFIELD_TEST_JAVA") else {
        return;
    };
    let classes = std::env::var("BLOCKFIELD_TEST_CLASSES").unwrap();
    let root = temp("java-online-arguments");
    let service = Service::new(root.join("data")).unwrap();
    let mut plan = empty_plan(&root);
    let secret = "dummy-java-test-token";
    let target = root.join("中文 空格/测试文件.txt");
    let cp = format!("{};{}", classes, "nonexistent-library.jar;".repeat(1500));
    plan.args = vec![
        "-Dfile.encoding=UTF-8".into(),
        "-Dstdout.encoding=UTF-8".into(),
        "-Dsun.stdout.encoding=UTF-8".into(),
        "-cp".into(),
        cp,
        "ArgumentProbe".into(),
        target.to_string_lossy().into_owned(),
        "--accessToken".into(),
        secret.into(),
        "中文参数".into(),
    ];
    plan.sensitive_token = Some(secret.into());
    plan.entry_index = 2;
    plan.entry_index = 5;
    let (args, _) = service
        .invocation(&plan, &json!({"major":22,"encoding":"GBK"}))
        .unwrap();
    let mut command = std::process::Command::new(java);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(text.contains("WRITABLE=true"));
    assert!(text.contains(secret));
    assert!(text.contains("中文参数"), "{text}");
}
#[test]
fn online_game_arguments_use_real_role_uuid_and_type() {
    let root = temp("online-role");
    let game = root.join(".minecraft");
    let dir = game.join("versions/Test");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("Test.json"),json!({"id":"Test","mainClass":"Main","libraries":[],"arguments":{"jvm":["-cp","${classpath}"],"game":["--username","${auth_player_name}","--uuid","${auth_uuid}","--accessToken","${auth_access_token}","--userType","${user_type}","--xuid","${auth_xuid}","--clientId","${clientid}"]}}).to_string()).unwrap();
    let service = Service::new(root.join("data")).unwrap();
    service.add_directory(game.to_str().unwrap()).unwrap();
    service.save(json!({"versionId":"Test"})).unwrap();
    let identity = crate::auth::Identity {
        name: "GenuinePlayer".into(),
        uuid: "0123456789abcdef0123456789abcdef".into(),
        access_token: "dummy-private-token".into(),
        client_id: "530c101a-f756-496e-8099-6addb6ed06ed".into(),
        xuid: "12345678".into(),
    };
    let plan = service.build_plan_with_identity(Some(&identity)).unwrap();
    for pair in [
        ("--username", "GenuinePlayer"),
        ("--uuid", "0123456789abcdef0123456789abcdef"),
        ("--accessToken", "dummy-private-token"),
        ("--userType", "msa"),
        ("--xuid", "12345678"),
    ] {
        assert!(plan
            .args
            .windows(2)
            .any(|a| a[0] == pair.0 && a[1] == pair.1));
    }
    assert_eq!(service.settings()["username"], "Explorer");
    let offline = service.build_plan().unwrap();
    assert!(offline
        .args
        .windows(2)
        .any(|a| a[0] == "--accessToken" && a[1] == "0"));
}
#[test]
fn long_arguments_preserve_chinese_and_legacy_classpath() {
    let root = temp("long-arguments");
    let service = Service::new(root.join("data")).unwrap();
    let mut plan = empty_plan(&root);
    plan.args = vec![
        "-cp".into(),
        "long-library.jar;".repeat(1600),
        "ArgumentProbe".into(),
        "中文 空格 路径".into(),
    ];
    let (args, cp) = service
        .invocation(&plan, &json!({"major":22,"encoding":"GBK"}))
        .unwrap();
    assert!(cp.is_none());
    assert!(args[0].starts_with('@'));
    let bytes = fs::read(args[0].trim_start_matches('@')).unwrap();
    let (decoded, _, errors) = encoding_rs::GBK.decode(&bytes);
    assert!(!errors);
    assert!(decoded.contains("中文 空格 路径"));
    let (args, cp) = service.invocation(&plan, &json!({"major":8})).unwrap();
    assert_eq!(cp.unwrap(), plan.args[1]);
    assert_eq!(args, vec!["ArgumentProbe", "中文 空格 路径"]);
}
#[test]
fn legacy_resources_are_materialized_in_both_layouts() {
    let root = temp("legacy-resources");
    let service = Service::new(root.join("data")).unwrap();
    let plan = empty_plan(&root);
    let hash = "0123456789012345678901234567890123456789";
    fs::create_dir_all(root.join("assets/indexes")).unwrap();
    fs::create_dir_all(root.join("assets/objects/01")).unwrap();
    fs::write(root.join("assets/objects/01").join(hash), b"asset content").unwrap();
    for flag in ["virtual", "map_to_resources"] {
        let index = json!({flag:true,"objects":{"sounds/test.ogg":{"hash":hash}}});
        fs::write(root.join("assets/indexes/legacy.json"), index.to_string()).unwrap();
        service.prepare_assets(&plan).unwrap();
        let target = if flag == "virtual" {
            root.join("assets/virtual/legacy")
        } else {
            plan.game_dir.join("resources")
        };
        assert_eq!(
            fs::read(target.join("sounds/test.ogg")).unwrap(),
            b"asset content"
        );
    }
}
#[test]
fn native_dlls_are_flattened_and_non_dll_entries_are_ignored() {
    use std::io::Write;
    let root = temp("native-extraction");
    let jar = root.join("native.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar).unwrap());
    for (name, data) in [
        ("folder/example.dll", b"dll".as_slice()),
        ("META-INF/MANIFEST.MF", b"metadata".as_slice()),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
    let target = root.join("natives");
    fs::create_dir_all(&target).unwrap();
    assert_eq!(extract_natives(&jar, &target).unwrap(), 1);
    assert_eq!(fs::read(target.join("example.dll")).unwrap(), b"dll");
    assert!(!target.join("META-INF").exists());
}
