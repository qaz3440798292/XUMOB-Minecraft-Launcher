use super::*;
use std::collections::VecDeque;
const CLIENT: &str = "530c101a-f756-496e-8099-6addb6ed06ed";
const UUID: &str = "0123456789abcdef0123456789abcdef";
#[test]
fn cancellation_while_network_reply_arrives_does_not_commit_credentials() {
    struct Delayed {
        inner: Arc<Mock>,
        started: Arc<std::sync::Barrier>,
        release: Arc<std::sync::Barrier>,
    }
    impl Transport for Delayed {
        fn send(&self, endpoint: Endpoint, body: Body, bearer: Option<&str>) -> R<Reply> {
            if endpoint == Endpoint::Xbox {
                self.started.wait();
                self.release.wait();
            }
            self.inner.send(endpoint, body, bearer)
        }
    }
    let started = Arc::new(std::sync::Barrier::new(2));
    let release = Arc::new(std::sync::Barrier::new(2));
    let manager = AuthManager::with_transport(
        temp(),
        Arc::new(Delayed {
            inner: Mock::new(signed_in_replies()),
            started: started.clone(),
            release: release.clone(),
        }),
    )
    .unwrap();
    let c = manager.start().unwrap();
    ready(&manager);
    let id = field(&c, "sessionId").to_string();
    let worker = manager.clone();
    let thread = std::thread::spawn(move || worker.poll(&id));
    started.wait();
    manager.cancel();
    release.wait();
    assert!(thread.join().unwrap().is_err());
    assert_eq!(manager.summary()["signedIn"], false);
    assert!(!manager.directory.join("account-session.bin").exists());
}
#[test]
fn rotated_refresh_is_retained_if_later_services_fail() {
    let manager = AuthManager::with_transport(
        temp(),
        Mock::new(vec![
            ok(
                Endpoint::Microsoft,
                json!({"access_token":"new-msa","refresh_token":"rotated-refresh"}),
            ),
            fail(Endpoint::Xbox, 503, json!({})),
        ]),
    )
    .unwrap();
    cached(&manager, true);
    assert!(manager.identity_for_launch().is_err());
    let restored =
        AuthManager::with_transport(manager.directory.clone(), Mock::new(vec![])).unwrap();
    assert_eq!(
        restored
            .state
            .lock()
            .unwrap()
            .session
            .as_ref()
            .unwrap()
            .refresh_token,
        "rotated-refresh"
    );
}
struct Mock {
    replies: Mutex<VecDeque<(Endpoint, Reply)>>,
    calls: Mutex<Vec<(Endpoint, Value)>>,
}
impl Mock {
    fn new(replies: Vec<(Endpoint, Reply)>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into()),
            calls: Mutex::new(vec![]),
        })
    }
}
impl Transport for Mock {
    fn send(&self, endpoint: Endpoint, body: Body, bearer: Option<&str>) -> R<Reply> {
        let data = match body {
            Body::Json(v) => v,
            Body::Form(fields) => fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), Value::String(v)))
                .collect(),
            Body::Empty => json!({"bearer":bearer}),
        };
        self.calls.lock().unwrap().push((endpoint, data));
        let (expected, reply) = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected authentication request");
        assert_eq!(endpoint, expected);
        Ok(reply)
    }
}
fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "blockfield-auth-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn ok(endpoint: Endpoint, data: Value) -> (Endpoint, Reply) {
    (endpoint, Reply { status: 200, data })
}
fn fail(endpoint: Endpoint, status: u16, data: Value) -> (Endpoint, Reply) {
    (endpoint, Reply { status, data })
}
fn device() -> (Endpoint, Reply) {
    ok(
        Endpoint::Device,
        json!({"device_code":"private-device-code","user_code":"ABCD-EFGH","verification_uri":"https://www.microsoft.com/devicelogin","expires_in":900,"interval":5}),
    )
}
fn xbox() -> Vec<(Endpoint, Reply)> {
    vec![
        ok(
            Endpoint::Xbox,
            json!({"Token":"xbox-test-token","DisplayClaims":{"xui":[{"uhs":"1234"}]}}),
        ),
        ok(
            Endpoint::Xsts,
            json!({"Token":"xsts-test-token","DisplayClaims":{"xui":[{"uhs":"1234","xid":"12345678"}]}}),
        ),
        ok(
            Endpoint::Minecraft,
            json!({"access_token":"minecraft-test-secret","expires_in":3600,"username":"12345678"}),
        ),
        license(),
        profile_reply("GenuinePlayer"),
    ]
}
fn license() -> (Endpoint, Reply) {
    ok(
        Endpoint::License,
        json!({"items":[{"name":"game_minecraft"}]}),
    )
}
fn profile_reply(name: &str) -> (Endpoint, Reply) {
    ok(Endpoint::Profile, json!({"id":UUID,"name":name}))
}
fn signed_in_replies() -> Vec<(Endpoint, Reply)> {
    let mut replies = vec![
        device(),
        ok(
            Endpoint::Microsoft,
            json!({"access_token":"microsoft-test-secret","refresh_token":"refresh-test-secret"}),
        ),
    ];
    replies.extend(xbox());
    replies
}
fn ready(manager: &AuthManager) {
    manager.state.lock().unwrap().pending.as_mut().unwrap().next = Instant::now();
}
fn login(manager: &AuthManager) {
    let challenge = manager.start().unwrap();
    ready(manager);
    assert_eq!(
        manager.poll(field(&challenge, "sessionId")).unwrap()["status"],
        "complete"
    );
}
fn cached(manager: &AuthManager, expired: bool) {
    let mut state = manager.state.lock().unwrap();
    let session = Session {
        client_id: CLIENT.into(),
        refresh_token: "old-refresh-secret".into(),
        access_token: "old-minecraft-secret".into(),
        expires_at: epoch() + if expired { -10 } else { 3600 },
        profile: Profile {
            id: UUID.into(),
            name: "PreviousName".into(),
            xuid: "12345678".into(),
        },
    };
    state.config.profile = Some(session.profile.clone());
    state.config.mode = "microsoft".into();
    manager.save_session(&session).unwrap();
    state.session = Some(session);
    manager.save_config(&state).unwrap();
}

#[test]
fn complete_authentication_uses_official_identity_and_encrypted_storage() {
    let dir = temp();
    let network = Mock::new(signed_in_replies());
    let manager = AuthManager::with_transport(dir.clone(), network.clone()).unwrap();
    login(&manager);
    let summary = manager.summary().to_string();
    assert!(summary.contains("GenuinePlayer"));
    assert!(!summary.contains("test-secret"));
    assert!(!summary.contains("device-code"));
    let preview = manager.preview().unwrap().unwrap();
    assert_eq!(preview.uuid, UUID);
    assert_eq!(preview.name, "GenuinePlayer");
    assert_eq!(preview.variables()["user_type"], "msa");
    assert_eq!(preview.access_token, "0");
    let encrypted = fs::read(dir.join("account-session.bin")).unwrap();
    assert!(!String::from_utf8_lossy(&encrypted).contains("refresh-test-secret"));
    let plain = unprotect(&encrypted).unwrap();
    assert!(String::from_utf8_lossy(&plain).contains("refresh-test-secret"));
    let restored = AuthManager::with_transport(dir.clone(), Mock::new(vec![])).unwrap();
    assert_eq!(restored.summary()["signedIn"], true);
    assert_eq!(restored.summary()["mode"], "microsoft");
    let calls = network.calls.lock().unwrap();
    assert_eq!(calls[0].1["scope"], SCOPE);
    assert_eq!(
        calls[2].1["Properties"]["RpsTicket"],
        "d=microsoft-test-secret"
    );
    assert_eq!(calls[4].1["identityToken"], "XBL3.0 x=1234;xsts-test-token");
    drop(calls);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn polling_enforces_interval_and_slow_down() {
    let network = Mock::new(vec![
        device(),
        fail(Endpoint::Microsoft, 400, json!({"error":"slow_down"})),
        fail(
            Endpoint::Microsoft,
            400,
            json!({"error":"authorization_pending"}),
        ),
    ]);
    let manager = AuthManager::with_transport(temp(), network.clone()).unwrap();
    let c = manager.start().unwrap();
    let id = field(&c, "sessionId");
    assert_eq!(manager.poll(id).unwrap()["status"], "pending");
    assert_eq!(network.calls.lock().unwrap().len(), 1);
    ready(&manager);
    assert_eq!(manager.poll(id).unwrap()["retryAfter"], 10);
    manager.poll(id).unwrap();
    assert_eq!(network.calls.lock().unwrap().len(), 2);
    ready(&manager);
    assert_eq!(manager.poll(id).unwrap()["retryAfter"], 10);
}
#[test]
fn denial_clears_pending_and_exposes_no_error_secrets() {
    let manager=AuthManager::with_transport(temp(),Mock::new(vec![device(),fail(Endpoint::Microsoft,400,json!({"error":"authorization_declined","error_description":"do-not-display-this-secret"}))])).unwrap();
    let c = manager.start().unwrap();
    ready(&manager);
    let error = manager.poll(field(&c, "sessionId")).unwrap_err();
    assert!(error.contains("拒绝"));
    assert!(!error.contains("secret"));
    assert_eq!(manager.summary()["pending"], false);
}
#[test]
fn expired_challenge_and_cancel_cannot_sign_in() {
    let manager = AuthManager::with_transport(temp(), Mock::new(vec![device(), device()])).unwrap();
    let c = manager.start().unwrap();
    manager
        .state
        .lock()
        .unwrap()
        .pending
        .as_mut()
        .unwrap()
        .expires = Instant::now();
    assert!(manager.poll(field(&c, "sessionId")).is_err());
    let c = manager.start().unwrap();
    manager.cancel();
    assert!(manager.poll(field(&c, "sessionId")).is_err());
    assert_eq!(manager.summary()["signedIn"], false);
    assert!(!manager.directory.join("account-session.bin").exists());
}
#[test]
fn untrusted_browser_urls_and_invalid_client_are_rejected() {
    for uri in [
        "https://evil.example/devicelogin",
        "http://microsoft.com/devicelogin",
        "https://microsoft.com@evil.example/",
        "https://microsoft.com:8443/devicelogin",
    ] {
        assert!(safe_uri(uri).is_err());
    }
    let manager = AuthManager::with_transport(temp(), Mock::new(vec![])).unwrap();
    assert!(manager.configure("not-a-guid").is_err());
    assert!(manager
        .configure("00000000-0000-0000-0000-000000000000")
        .is_err());
}
#[test]
fn missing_entitlement_never_commits_session() {
    let mut replies = signed_in_replies();
    replies.truncate(5);
    replies.push(ok(Endpoint::License, json!({"items":[]})));
    let manager = AuthManager::with_transport(temp(), Mock::new(replies)).unwrap();
    let c = manager.start().unwrap();
    ready(&manager);
    assert!(manager
        .poll(field(&c, "sessionId"))
        .unwrap_err()
        .contains("授权"));
    assert_eq!(manager.summary()["signedIn"], false);
    assert!(!manager.directory.join("account-session.bin").exists());
}
#[test]
fn refresh_rotates_credentials_and_updates_role_name() {
    let mut replies = vec![ok(
        Endpoint::Microsoft,
        json!({"access_token":"new-microsoft-secret","refresh_token":"new-refresh-secret"}),
    )];
    replies.extend(xbox());
    let network = Mock::new(replies);
    let manager = AuthManager::with_transport(temp(), network.clone()).unwrap();
    cached(&manager, true);
    let identity = manager.identity_for_launch().unwrap().unwrap();
    assert_eq!(identity.name, "GenuinePlayer");
    assert_eq!(identity.uuid, UUID);
    assert_eq!(identity.access_token, "minecraft-test-secret");
    assert_eq!(identity.client_id, CLIENT);
    assert_eq!(
        network.calls.lock().unwrap()[0].1["refresh_token"],
        "old-refresh-secret"
    );
    let session = manager.state.lock().unwrap().session.clone().unwrap();
    assert_eq!(session.refresh_token, "new-refresh-secret");
}
#[test]
fn revoked_refresh_requires_login_instead_of_offline_fallback() {
    let manager = AuthManager::with_transport(
        temp(),
        Mock::new(vec![fail(
            Endpoint::Microsoft,
            400,
            json!({"error":"invalid_grant"}),
        )]),
    )
    .unwrap();
    cached(&manager, true);
    assert!(manager.identity_for_launch().is_err());
    assert_eq!(manager.summary()["mode"], "microsoft");
    assert_eq!(manager.summary()["needsLogin"], true);
    assert_eq!(manager.summary()["signedIn"], false);
    assert!(manager.identity_for_launch().is_err());
}
#[test]
fn valid_token_still_checks_ownership_and_current_profile() {
    let manager = AuthManager::with_transport(
        temp(),
        Mock::new(vec![license(), profile_reply("RenamedPlayer")]),
    )
    .unwrap();
    cached(&manager, false);
    assert_eq!(
        manager.identity_for_launch().unwrap().unwrap().name,
        "RenamedPlayer"
    );
    let denied = AuthManager::with_transport(
        temp(),
        Mock::new(vec![ok(Endpoint::License, json!({"items":[]}))]),
    )
    .unwrap();
    cached(&denied, false);
    assert!(denied.identity_for_launch().is_err());
    assert_eq!(denied.summary()["mode"], "microsoft");
}
#[test]
fn offline_switch_keeps_login_and_logout_removes_it() {
    let manager = AuthManager::with_transport(temp(), Mock::new(vec![])).unwrap();
    cached(&manager, false);
    manager.select("offline").unwrap();
    assert!(manager.identity_for_launch().unwrap().is_none());
    assert!(manager.directory.join("account-session.bin").exists());
    manager.select("microsoft").unwrap();
    assert_eq!(manager.summary()["mode"], "microsoft");
    manager.logout().unwrap();
    assert!(!manager.directory.join("account-session.bin").exists());
    assert_eq!(manager.summary()["profile"], Value::Null);
    assert!(manager.select("microsoft").is_err());
}
#[test]
fn corrupted_vault_requests_reauthentication() {
    let dir = temp();
    let manager = AuthManager::with_transport(dir.clone(), Mock::new(vec![])).unwrap();
    cached(&manager, false);
    fs::write(dir.join("account-session.bin"), b"damaged").unwrap();
    let restored = AuthManager::with_transport(dir, Mock::new(vec![])).unwrap();
    assert_eq!(restored.summary()["needsLogin"], true);
    assert!(restored.identity_for_launch().is_err());
}
#[test]
fn token_strings_are_redacted_before_logging() {
    let manager = AuthManager::with_transport(temp(), Mock::new(vec![])).unwrap();
    cached(&manager, false);
    let line = manager.redact("old-minecraft-secret old-refresh-secret");
    assert_eq!(line, "[已隐藏] [已隐藏]");
}
#[test]
fn changed_xbox_user_hash_is_rejected() {
    let mut replies = signed_in_replies();
    replies.truncate(4);
    replies[3] = ok(
        Endpoint::Xsts,
        json!({"Token":"test-xsts","DisplayClaims":{"xui":[{"uhs":"another-user"}]}}),
    );
    let manager = AuthManager::with_transport(temp(), Mock::new(replies)).unwrap();
    let c = manager.start().unwrap();
    ready(&manager);
    assert!(manager
        .poll(field(&c, "sessionId"))
        .unwrap_err()
        .contains("不一致"));
    assert_eq!(manager.summary()["signedIn"], false);
}
#[test]
fn xbox_and_application_errors_are_explained() {
    assert!(response(
        Reply {
            status: 401,
            data: json!({"XErr":2148916238u64})
        },
        Endpoint::Xsts
    )
    .unwrap_err()
    .contains("监护人"));
    assert!(response(
        Reply {
            status: 403,
            data: json!({"error":"Forbidden","errorMessage":"Invalid app registration, see https://aka.ms/AppRegInfo for more information"})
        },
        Endpoint::Minecraft
    )
    .unwrap_err()
    .contains("Client ID"));
}

#[test]
fn minecraft_forbidden_does_not_assume_application_permission() {
    for data in [json!({"error":"Forbidden"}), Value::Null] {
        let error = response(Reply { status: 403, data }, Endpoint::Minecraft).unwrap_err();
        assert!(error.contains("无法确定"));
        assert!(error.contains("HTTP 403"));
        assert!(!error.contains("明确拒绝了此应用注册"));
    }
}

#[test]
fn minecraft_registration_diagnostics_exclude_response_secrets() {
    let error = response(
        Reply {
            status: 403,
            data: json!({
                "errorMessage":"Invalid app registration secret-response-token",
                "access_token":"secret-access-token",
                "requestId":"01234567-89ab-4cde-8fab-0123456789ab"
            }),
        },
        Endpoint::Minecraft,
    )
    .unwrap_err();
    assert!(error.contains("Invalid app registration"));
    assert!(error.contains("01234567-89ab-4cde-8fab-0123456789ab"));
    assert!(!error.contains("secret"));
    let error = response(
        Reply {
            status: 403,
            data: json!({
                "message":"secret-server-message", "requestId":"secret-request-token"
            }),
        },
        Endpoint::Minecraft,
    )
    .unwrap_err();
    assert!(!error.contains("secret"));
}
