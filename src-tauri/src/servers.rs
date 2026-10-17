use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::{Cursor, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type R<T> = Result<T, String>;
const MAX_NBT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PACKET_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(u32),
    Double(u64),
    ByteArray(Vec<i8>),
    String(String),
    List(u8, Vec<Tag>),
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}
impl Tag {
    fn id(&self) -> u8 {
        match self {
            Self::Byte(_) => 1,
            Self::Short(_) => 2,
            Self::Int(_) => 3,
            Self::Long(_) => 4,
            Self::Float(_) => 5,
            Self::Double(_) => 6,
            Self::ByteArray(_) => 7,
            Self::String(_) => 8,
            Self::List(_, _) => 9,
            Self::Compound(_) => 10,
            Self::IntArray(_) => 11,
            Self::LongArray(_) => 12,
        }
    }
    fn get(&self, key: &str) -> Option<&Tag> {
        match self {
            Self::Compound(values) => values.iter().find(|(name, _)| name == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn set(&mut self, key: &str, value: Tag) -> R<()> {
        let Self::Compound(values) = self else {
            return Err("服务器列表数据格式无效。".into());
        };
        if let Some((_, old)) = values.iter_mut().find(|(name, _)| name == key) {
            *old = value;
        } else {
            values.push((key.to_owned(), value));
        }
        Ok(())
    }
    fn string(&self) -> Option<&str> {
        if let Self::String(value) = self {
            Some(value)
        } else {
            None
        }
    }
}

fn read_exact<const N: usize>(input: &mut Cursor<&[u8]>) -> R<[u8; N]> {
    let mut bytes = [0; N];
    input
        .read_exact(&mut bytes)
        .map_err(|_| "服务器列表文件已损坏。")?;
    Ok(bytes)
}
fn read_u8(input: &mut Cursor<&[u8]>) -> R<u8> {
    Ok(read_exact::<1>(input)?[0])
}
fn read_string(input: &mut Cursor<&[u8]>) -> R<String> {
    let size = u16::from_be_bytes(read_exact(input)?) as usize;
    let mut bytes = vec![0; size];
    input
        .read_exact(&mut bytes)
        .map_err(|_| "服务器列表文件已损坏。")?;
    String::from_utf8(bytes).map_err(|_| "服务器列表包含无效文本。".into())
}
fn read_len(input: &mut Cursor<&[u8]>) -> R<usize> {
    let value = i32::from_be_bytes(read_exact(input)?);
    if !(0..=1_000_000).contains(&value) {
        return Err("服务器列表数组长度无效。".into());
    }
    Ok(value as usize)
}
fn read_payload(input: &mut Cursor<&[u8]>, id: u8, depth: usize) -> R<Tag> {
    if depth > 64 {
        return Err("服务器列表嵌套层级过深。".into());
    }
    Ok(match id {
        1 => Tag::Byte(read_u8(input)? as i8),
        2 => Tag::Short(i16::from_be_bytes(read_exact(input)?)),
        3 => Tag::Int(i32::from_be_bytes(read_exact(input)?)),
        4 => Tag::Long(i64::from_be_bytes(read_exact(input)?)),
        5 => Tag::Float(u32::from_be_bytes(read_exact(input)?)),
        6 => Tag::Double(u64::from_be_bytes(read_exact(input)?)),
        7 => {
            let size = read_len(input)?;
            let mut bytes = vec![0; size];
            input
                .read_exact(&mut bytes)
                .map_err(|_| "服务器列表文件已损坏。")?;
            Tag::ByteArray(bytes.into_iter().map(|v| v as i8).collect())
        }
        8 => Tag::String(read_string(input)?),
        9 => {
            let child = read_u8(input)?;
            if child > 12 {
                return Err("服务器列表包含未知数据类型。".into());
            }
            let size = read_len(input)?;
            if child == 0 && size != 0 {
                return Err("服务器列表数据格式无效。".into());
            }
            let mut values = Vec::with_capacity(size.min(4096));
            for _ in 0..size {
                values.push(read_payload(input, child, depth + 1)?);
            }
            Tag::List(child, values)
        }
        10 => {
            let mut values = Vec::new();
            loop {
                let child = read_u8(input)?;
                if child == 0 {
                    break;
                }
                if child > 12 {
                    return Err("服务器列表包含未知数据类型。".into());
                }
                let name = read_string(input)?;
                values.push((name, read_payload(input, child, depth + 1)?));
                if values.len() > 100_000 {
                    return Err("服务器列表条目过多。".into());
                }
            }
            Tag::Compound(values)
        }
        11 => {
            let size = read_len(input)?;
            let mut values = Vec::with_capacity(size.min(4096));
            for _ in 0..size {
                values.push(i32::from_be_bytes(read_exact(input)?));
            }
            Tag::IntArray(values)
        }
        12 => {
            let size = read_len(input)?;
            let mut values = Vec::with_capacity(size.min(4096));
            for _ in 0..size {
                values.push(i64::from_be_bytes(read_exact(input)?));
            }
            Tag::LongArray(values)
        }
        _ => return Err("服务器列表包含未知数据类型。".into()),
    })
}
fn read_nbt(bytes: &[u8]) -> R<(String, Tag)> {
    let mut input = Cursor::new(bytes);
    if read_u8(&mut input)? != 10 {
        return Err("服务器列表根数据不是 NBT 复合标签。".into());
    }
    let name = read_string(&mut input)?;
    let root = read_payload(&mut input, 10, 0)?;
    if input.position() != bytes.len() as u64 {
        return Err("服务器列表文件包含无法识别的尾部数据。".into());
    }
    Ok((name, root))
}
fn write_string(output: &mut Vec<u8>, value: &str) -> R<()> {
    let bytes = value.as_bytes();
    let size = u16::try_from(bytes.len()).map_err(|_| "服务器列表文本过长。")?;
    output.extend(size.to_be_bytes());
    output.extend(bytes);
    Ok(())
}
fn write_len(output: &mut Vec<u8>, size: usize) -> R<()> {
    output.extend(
        i32::try_from(size)
            .map_err(|_| "服务器列表数组过大。")?
            .to_be_bytes(),
    );
    Ok(())
}
fn write_payload(output: &mut Vec<u8>, tag: &Tag, depth: usize) -> R<()> {
    if depth > 64 {
        return Err("服务器列表嵌套层级过深。".into());
    }
    match tag {
        Tag::Byte(v) => output.push(*v as u8),
        Tag::Short(v) => output.extend(v.to_be_bytes()),
        Tag::Int(v) => output.extend(v.to_be_bytes()),
        Tag::Long(v) => output.extend(v.to_be_bytes()),
        Tag::Float(v) => output.extend(v.to_be_bytes()),
        Tag::Double(v) => output.extend(v.to_be_bytes()),
        Tag::ByteArray(values) => {
            write_len(output, values.len())?;
            output.extend(values.iter().map(|v| *v as u8));
        }
        Tag::String(v) => write_string(output, v)?,
        Tag::List(id, values) => {
            output.push(*id);
            write_len(output, values.len())?;
            for value in values {
                if value.id() != *id {
                    return Err("服务器列表中的 NBT 列表类型不一致。".into());
                }
                write_payload(output, value, depth + 1)?;
            }
        }
        Tag::Compound(values) => {
            for (name, value) in values {
                output.push(value.id());
                write_string(output, name)?;
                write_payload(output, value, depth + 1)?;
            }
            output.push(0);
        }
        Tag::IntArray(values) => {
            write_len(output, values.len())?;
            for v in values {
                output.extend(v.to_be_bytes());
            }
        }
        Tag::LongArray(values) => {
            write_len(output, values.len())?;
            for v in values {
                output.extend(v.to_be_bytes());
            }
        }
    }
    Ok(())
}
fn write_nbt(name: &str, root: &Tag) -> R<Vec<u8>> {
    if root.id() != 10 {
        return Err("服务器列表根数据不是复合标签。".into());
    }
    let mut bytes = vec![10];
    write_string(&mut bytes, name)?;
    write_payload(&mut bytes, root, 0)?;
    Ok(bytes)
}

fn new_root() -> Tag {
    Tag::Compound(vec![("servers".into(), Tag::List(10, Vec::new()))])
}
fn read_file(path: &Path) -> R<(String, Tag, bool)> {
    if !path.exists() {
        return Ok((String::new(), new_root(), false));
    }
    if fs::metadata(path)
        .map_err(|_| "无法读取当前版本的服务器列表。")?
        .len()
        > MAX_NBT_BYTES
    {
        return Err("服务器列表文件过大，已停止读取。".into());
    }
    let bytes = fs::read(path).map_err(|_| "无法读取当前版本的服务器列表。")?;
    let compressed = bytes.starts_with(&[0x1f, 0x8b]);
    let decoded = if compressed {
        let decoder = GzDecoder::new(bytes.as_slice());
        let mut output = Vec::new();
        decoder
            .take(MAX_NBT_BYTES + 1)
            .read_to_end(&mut output)
            .map_err(|_| "服务器列表压缩数据已损坏。")?;
        if output.len() as u64 > MAX_NBT_BYTES {
            return Err("服务器列表解压后过大。".into());
        }
        output
    } else {
        bytes
    };
    let (name, mut root) = read_nbt(&decoded)?;
    if root.get("servers").is_none() {
        root.set("servers", Tag::List(10, Vec::new()))?;
    }
    Ok((name, root, compressed))
}
fn save_file(path: &Path, name: &str, root: &Tag, compressed: bool) -> R<()> {
    let bytes = write_nbt(name, root)?;
    let bytes = if compressed {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&bytes)
            .map_err(|_| "无法压缩服务器列表。")?;
        encoder.finish().map_err(|_| "无法压缩服务器列表。")?
    } else {
        bytes
    };
    let parent = path.parent().ok_or("无法确定服务器列表目录。")?;
    fs::create_dir_all(parent).map_err(|_| "无法创建游戏目录。")?;
    let temporary = path.with_extension(format!("dat.{}.tmp", std::process::id()));
    let mut file = File::create(&temporary).map_err(|_| "无法创建服务器列表临时文件。")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "无法保存服务器列表。")?;
    drop(file);
    fs::rename(&temporary, path).map_err(|_| {
        let _ = fs::remove_file(&temporary);
        "无法更新服务器列表文件。".to_string()
    })
}
fn entries(root: &Tag) -> R<&Vec<Tag>> {
    match root.get("servers") {
        Some(Tag::List(10, values)) => Ok(values),
        Some(_) => Err("服务器列表的 servers 项格式无效。".into()),
        None => Err("服务器列表缺少 servers 项。".into()),
    }
}
fn entries_mut(root: &mut Tag) -> R<&mut Vec<Tag>> {
    let Tag::Compound(values) = root else {
        return Err("服务器列表数据格式无效。".into());
    };
    match values.iter_mut().find(|(name, _)| name == "servers") {
        Some((_, Tag::List(10, servers))) => Ok(servers),
        _ => Err("服务器列表的 servers 项格式无效。".into()),
    }
}
fn server_entry(name: &str, address: &str) -> Tag {
    Tag::Compound(vec![
        ("name".into(), Tag::String(name.into())),
        ("ip".into(), Tag::String(address.into())),
        ("acceptTextures".into(), Tag::Byte(0)),
    ])
}
fn entry_string<'a>(entry: &'a Tag, key: &str) -> &'a str {
    entry.get(key).and_then(Tag::string).unwrap_or("")
}
fn server_view(entry: &Tag) -> Value {
    let icon = entry_string(entry, "icon");
    json!({
        "name":entry_string(entry, "name"),
        "address":entry_string(entry, "ip"),
        "icon":if icon.starts_with("data:image/png;base64,") && icon.len() <= 128 * 1024 { icon } else { "" },
        "hidden":matches!(entry.get("hidden"), Some(Tag::Byte(value)) if *value != 0),
        "acceptTextures":matches!(entry.get("acceptTextures"), Some(Tag::Byte(value)) if *value != 0)
    })
}
fn validate_name(name: &str) -> R<()> {
    let value = name.trim();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err("请输入有效的服务器名称（最多 256 字节）。".into());
    }
    Ok(())
}
pub(crate) fn parse_address(address: &str) -> R<(String, u16)> {
    let value = address.trim();
    if value.is_empty()
        || value.len() > 255
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(
            "请输入有效服务器地址，例如 play.example.com 或 play.example.com:25565。".into(),
        );
    }
    let (host, port) = if let Some(rest) = value.strip_prefix('[') {
        let (host, suffix) = rest.split_once(']').ok_or("IPv6 地址格式无效。")?;
        if suffix.is_empty() {
            (host, 25565)
        } else {
            let raw = suffix.strip_prefix(':').ok_or("IPv6 端口格式无效。")?;
            (host, raw.parse::<u16>().map_err(|_| "服务器端口无效。")?)
        }
    } else if value.matches(':').count() == 1 {
        let (host, raw) = value.rsplit_once(':').unwrap();
        (host, raw.parse::<u16>().map_err(|_| "服务器端口无效。")?)
    } else {
        (value, 25565)
    };
    if host.is_empty() || port == 0 || host.len() > 255 || host.contains('/') || host.contains('\\')
    {
        return Err("服务器地址或端口无效。".into());
    }
    Ok((host.into(), port))
}

fn put_varint(output: &mut Vec<u8>, mut value: i32) {
    loop {
        if value & !0x7f == 0 {
            output.push(value as u8);
            return;
        }
        output.push(((value & 0x7f) | 0x80) as u8);
        value = ((value as u32) >> 7) as i32;
    }
}
fn read_varint(input: &mut impl Read) -> R<i32> {
    let mut result = 0i32;
    for position in 0..5 {
        let mut byte = [0];
        input
            .read_exact(&mut byte)
            .map_err(|_| "服务器没有返回完整状态数据。")?;
        result |= ((byte[0] & 0x7f) as i32) << (7 * position);
        if byte[0] & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err("服务器返回了无效的数据长度。".into())
}
fn packet(payload: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(payload.len() + 5);
    put_varint(&mut result, payload.len() as i32);
    result.extend(payload);
    result
}
fn read_packet(stream: &mut TcpStream) -> R<Vec<u8>> {
    let length = read_varint(stream)?;
    if length <= 0 || length as usize > MAX_PACKET_BYTES {
        return Err("服务器状态数据大小无效。".into());
    }
    let mut payload = vec![0; length as usize];
    stream
        .read_exact(&mut payload)
        .map_err(|_| "读取服务器状态失败。")?;
    Ok(payload)
}
fn read_packet_string(payload: &[u8], offset: usize) -> R<String> {
    let mut cursor = Cursor::new(&payload[offset..]);
    let length = read_varint(&mut cursor)?;
    if length < 0 || length as usize > MAX_PACKET_BYTES {
        return Err("服务器 MOTD 数据大小无效。".into());
    }
    let mut bytes = vec![0; length as usize];
    cursor
        .read_exact(&mut bytes)
        .map_err(|_| "读取服务器 MOTD 失败。")?;
    String::from_utf8(bytes).map_err(|_| "服务器 MOTD 不是有效 UTF-8。".into())
}
fn write_varint_string(output: &mut Vec<u8>, value: &str) {
    put_varint(output, value.len() as i32);
    output.extend(value.as_bytes());
}

#[derive(Clone, Default)]
struct Style {
    color: Option<String>,
    bold: bool,
    italic: bool,
    underlined: bool,
    strikethrough: bool,
    obfuscated: bool,
}
fn game_color(code: char) -> Option<&'static str> {
    Some(match code.to_ascii_lowercase() {
        '0' => "#000000",
        '1' => "#0000aa",
        '2' => "#00aa00",
        '3' => "#00aaaa",
        '4' => "#aa0000",
        '5' => "#aa00aa",
        '6' => "#ffaa00",
        '7' => "#aaaaaa",
        '8' => "#555555",
        '9' => "#5555ff",
        'a' => "#55ff55",
        'b' => "#55ffff",
        'c' => "#ff5555",
        'd' => "#ff55ff",
        'e' => "#ffff55",
        'f' => "#ffffff",
        _ => return None,
    })
}
fn named_color(value: &str) -> Option<String> {
    if value.starts_with('#')
        && value.len() == 7
        && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Some(value.to_owned());
    }
    let code = match value {
        "black" => '0',
        "dark_blue" => '1',
        "dark_green" => '2',
        "dark_aqua" => '3',
        "dark_red" => '4',
        "dark_purple" => '5',
        "gold" => '6',
        "gray" => '7',
        "dark_gray" => '8',
        "blue" => '9',
        "green" => 'a',
        "aqua" => 'b',
        "red" => 'c',
        "light_purple" => 'd',
        "yellow" => 'e',
        "white" => 'f',
        _ => return None,
    };
    game_color(code).map(str::to_owned)
}
fn emit_text(text: &str, style: &Style, output: &mut Vec<Value>) {
    if text.is_empty() {
        return;
    }
    output.push(json!({
        "text":text, "color":style.color, "bold":style.bold, "italic":style.italic,
        "underlined":style.underlined, "strikethrough":style.strikethrough,
        "obfuscated":style.obfuscated
    }));
}
fn legacy_text(text: &str, inherited: &Style, output: &mut Vec<Value>) {
    let mut style = inherited.clone();
    let mut segment = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            if let Some(code) = chars.next() {
                if code.eq_ignore_ascii_case(&'x') {
                    let mut hex = String::with_capacity(6);
                    let mut lookahead = chars.clone();
                    let mut valid = true;
                    for _ in 0..6 {
                        if lookahead.next() != Some('§') {
                            valid = false;
                            break;
                        }
                        match lookahead.next() {
                            Some(digit) if digit.is_ascii_hexdigit() => hex.push(digit),
                            _ => {
                                valid = false;
                                break;
                            }
                        }
                    }
                    if valid {
                        emit_text(&segment, &style, output);
                        segment.clear();
                        style.color = Some(format!("#{hex}"));
                        chars = lookahead;
                        continue;
                    }
                }
                let colored = game_color(code);
                if colored.is_some()
                    || matches!(code.to_ascii_lowercase(), 'r' | 'k' | 'l' | 'm' | 'n' | 'o')
                {
                    emit_text(&segment, &style, output);
                    segment.clear();
                    match code.to_ascii_lowercase() {
                        'r' => style = inherited.clone(),
                        'k' => style.obfuscated = true,
                        'l' => style.bold = true,
                        'm' => style.strikethrough = true,
                        'n' => style.underlined = true,
                        'o' => style.italic = true,
                        _ => {
                            style = inherited.clone();
                            style.color = colored.map(str::to_owned);
                        }
                    }
                    continue;
                }
                segment.push(ch);
                segment.push(code);
            }
        } else {
            segment.push(ch);
        }
    }
    emit_text(&segment, &style, output);
}
fn flatten_component(component: &Value, inherited: &Style, output: &mut Vec<Value>, depth: usize) {
    if depth > 48 {
        return;
    }
    if let Some(parts) = component.as_array() {
        for part in parts {
            flatten_component(part, inherited, output, depth + 1);
        }
        return;
    }
    if let Some(text) = component.as_str() {
        legacy_text(text, inherited, output);
        return;
    }
    let Some(object) = component.as_object() else {
        return;
    };
    let mut style = inherited.clone();
    if let Some(color) = object
        .get("color")
        .and_then(Value::as_str)
        .and_then(named_color)
    {
        style.color = Some(color);
    }
    for (key, slot) in [
        ("bold", &mut style.bold),
        ("italic", &mut style.italic),
        ("underlined", &mut style.underlined),
        ("strikethrough", &mut style.strikethrough),
        ("obfuscated", &mut style.obfuscated),
    ] {
        if let Some(value) = object.get(key).and_then(Value::as_bool) {
            *slot = value;
        }
    }
    if let Some(text) = object.get("text").and_then(Value::as_str) {
        legacy_text(text, &style, output);
    } else if let Some(key) = object.get("translate").and_then(Value::as_str) {
        legacy_text(key, &style, output);
    } else if let Some(key) = object.get("keybind").and_then(Value::as_str) {
        legacy_text(key, &style, output);
    }
    if let Some(extra) = object.get("extra") {
        flatten_component(extra, &style, output, depth + 1);
    }
    if let Some(with) = object.get("with") {
        flatten_component(with, &style, output, depth + 1);
    }
}
fn motd_segments(description: &Value) -> Vec<Value> {
    let mut segments = Vec::new();
    flatten_component(description, &Style::default(), &mut segments, 0);
    if segments.is_empty() {
        segments.push(json!({"text":"","color":null}));
    }
    segments
}

fn ping(address: &str) -> Value {
    let (host, port) = match parse_address(address) {
        Ok(value) => value,
        Err(error) => return json!({"ok":false,"error":error,"latencyMs":null}),
    };
    let sockets = match (host.as_str(), port).to_socket_addrs() {
        Ok(addresses) => addresses,
        Err(_) => return json!({"ok":false,"error":"无法解析服务器地址。","latencyMs":null}),
    };
    let mut connected = None;
    for target in sockets {
        if let Ok(stream) = TcpStream::connect_timeout(&target, Duration::from_secs(4)) {
            connected = Some(stream);
            break;
        }
    }
    let Some(mut stream) = connected else {
        return json!({"ok":false,"error":"无法连接服务器，请检查地址、端口或网络。","latencyMs":null});
    };
    let timeout = Some(Duration::from_secs(5));
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);
    let started = Instant::now();
    let mut handshake = Vec::new();
    put_varint(&mut handshake, 0);
    put_varint(&mut handshake, -1);
    write_varint_string(&mut handshake, &host);
    handshake.extend(port.to_be_bytes());
    put_varint(&mut handshake, 1);
    if stream.write_all(&packet(&handshake)).is_err() || stream.write_all(&[1, 0]).is_err() {
        return json!({"ok":false,"error":"发送服务器状态请求失败。","latencyMs":null});
    }
    let response = match read_packet(&mut stream) {
        Ok(value) => value,
        Err(error) => return json!({"ok":false,"error":error,"latencyMs":null}),
    };
    let mut cursor = Cursor::new(response.as_slice());
    if read_varint(&mut cursor).ok() != Some(0) {
        return json!({"ok":false,"error":"服务器返回了无效状态响应。","latencyMs":null});
    }
    let status_text = match read_packet_string(&response, cursor.position() as usize) {
        Ok(value) => value,
        Err(error) => return json!({"ok":false,"error":error,"latencyMs":null}),
    };
    let status: Value = match serde_json::from_str(&status_text) {
        Ok(value) => value,
        Err(_) => return json!({"ok":false,"error":"服务器状态内容格式无效。","latencyMs":null}),
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let mut ping_payload = vec![1];
    ping_payload.extend(timestamp.to_be_bytes());
    if stream.write_all(&packet(&ping_payload)).is_err() {
        return json!({"ok":false,"error":"发送延迟检测请求失败。","latencyMs":null});
    }
    let pong = match read_packet(&mut stream) {
        Ok(value) => value,
        Err(error) => return json!({"ok":false,"error":error,"latencyMs":null}),
    };
    if pong.first() != Some(&1) || pong.len() != 9 {
        return json!({"ok":false,"error":"服务器返回了无效延迟响应。","latencyMs":null});
    }
    let favicon = status
        .get("favicon")
        .and_then(Value::as_str)
        .filter(|v| v.starts_with("data:image/png;base64,") && v.len() <= 128 * 1024)
        .unwrap_or("");
    json!({
        "ok":true,
        "latencyMs":started.elapsed().as_millis().min(u32::MAX as u128) as u32,
        "playersOnline":status["players"]["online"].as_u64(),
        "playersMax":status["players"]["max"].as_u64(),
        "motd":motd_segments(status.get("description").unwrap_or(&Value::Null)),
        "favicon":favicon,
        "version":status["version"]["name"].as_str().unwrap_or("")
    })
}

pub(crate) fn list(path: &Path) -> R<Value> {
    let (_, root, _) = read_file(path)?;
    Ok(json!(entries(&root)?
        .iter()
        .map(server_view)
        .collect::<Vec<_>>()))
}
pub(crate) fn add(path: &Path, name: &str, address: &str, compressed: bool) -> R<Value> {
    validate_name(name)?;
    parse_address(address)?;
    let (root_name, mut root, _) = read_file(path)?;
    let servers = entries_mut(&mut root)?;
    let normalized = address.trim();
    if let Some(existing) = servers
        .iter_mut()
        .find(|entry| entry_string(entry, "ip").eq_ignore_ascii_case(normalized))
    {
        existing.set("name", Tag::String(name.trim().into()))?;
    } else {
        if servers.len() >= 2000 {
            return Err("服务器列表已达到 2000 项上限。".into());
        }
        servers.push(server_entry(name.trim(), normalized));
    }
    save_file(path, &root_name, &root, compressed)?;
    list(path)
}
pub(crate) fn remove(path: &Path, address: &str, compressed: bool) -> R<Value> {
    let (root_name, mut root, _) = read_file(path)?;
    let servers = entries_mut(&mut root)?;
    let before = servers.len();
    servers.retain(|entry| !entry_string(entry, "ip").eq_ignore_ascii_case(address));
    if servers.len() == before {
        return Err("找不到要删除的服务器。".into());
    }
    save_file(path, &root_name, &root, compressed)?;
    list(path)
}
pub(crate) fn move_server(path: &Path, address: &str, offset: i32, compressed: bool) -> R<Value> {
    let (root_name, mut root, _) = read_file(path)?;
    let servers = entries_mut(&mut root)?;
    let index = servers
        .iter()
        .position(|entry| entry_string(entry, "ip").eq_ignore_ascii_case(address))
        .ok_or("找不到要移动的服务器。")?;
    let last = servers.len().saturating_sub(1) as i32;
    let next = (index as i32 + offset).clamp(0, last) as usize;
    if index != next {
        let server = servers.remove(index);
        servers.insert(next, server);
    }
    save_file(path, &root_name, &root, compressed)?;
    list(path)
}
pub(crate) fn ping_all(path: &Path, compressed: bool) -> R<Value> {
    let (root_name, mut root, _) = read_file(path)?;
    let servers = entries(&root)?.iter().map(server_view).collect::<Vec<_>>();
    let mut results = Vec::with_capacity(servers.len());
    for group in servers.chunks(16) {
        let shared_results = Arc::new(Mutex::new(vec![Value::Null; group.len()]));
        thread::scope(|scope| {
            for (index, server) in group.iter().enumerate() {
                let results = shared_results.clone();
                let address = server["address"].as_str().unwrap_or("").to_owned();
                scope.spawn(move || results.lock().unwrap()[index] = ping(&address));
            }
        });
        let mut group_results = Arc::try_unwrap(shared_results)
            .map_err(|_| "服务器状态检测未完成。".to_string())?
            .into_inner()
            .map_err(|_| "服务器状态检测失败。".to_string())?;
        results.append(&mut group_results);
    }
    let mut changed = false;
    for (entry, result) in entries_mut(&mut root)?.iter_mut().zip(&results) {
        if let Some(icon) = result["favicon"].as_str().filter(|icon| !icon.is_empty()) {
            entry.set("icon", Tag::String(icon.into()))?;
            changed = true;
        }
    }
    if changed {
        save_file(path, &root_name, &root, compressed)?;
    }
    Ok(Value::Array(
        servers
            .iter()
            .zip(results)
            .map(|(server, status)| {
                let mut view = server.clone();
                view["status"] = status;
                if view["status"]["favicon"]
                    .as_str()
                    .is_some_and(|icon| !icon.is_empty())
                {
                    view["icon"] = view["status"]["favicon"].clone();
                }
                view
            })
            .collect(),
    ))
}

pub(crate) fn normalize_format(path: &Path, compressed: bool) -> R<()> {
    if !path.exists() {
        return Ok(());
    }
    let (name, root, was_compressed) = read_file(path)?;
    if was_compressed != compressed {
        save_file(path, &name, &root, compressed)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nbt_round_trip_preserves_unknown_server_data() {
        let mut root = new_root();
        root.set(
            "unknown",
            Tag::Compound(vec![("value".into(), Tag::Long(42))]),
        )
        .unwrap();
        let mut servers = vec![server_entry("Example", "play.example.net")];
        servers[0].set("hidden", Tag::Byte(1)).unwrap();
        root.set("servers", Tag::List(10, servers)).unwrap();
        let encoded = write_nbt("root", &root).unwrap();
        let (name, decoded) = read_nbt(&encoded).unwrap();
        assert_eq!(name, "root");
        assert!(matches!(
            decoded.get("unknown").and_then(|tag| tag.get("value")),
            Some(Tag::Long(42))
        ));
        assert_eq!(
            server_view(&entries(&decoded).unwrap()[0])["address"],
            "play.example.net"
        );
    }
    #[test]
    fn servers_file_format_can_be_migrated_for_calendar_versions() {
        let path = std::env::temp_dir().join(format!(
            "xumob-servers-{}-{}.dat",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut root = new_root();
        entries_mut(&mut root)
            .unwrap()
            .push(server_entry("Hypixel", "mc.hypixel.net"));
        save_file(&path, "", &root, true).unwrap();
        assert!(fs::read(&path).unwrap().starts_with(&[0x1f, 0x8b]));

        normalize_format(&path, false).unwrap();
        let raw = fs::read(&path).unwrap();
        assert!(!raw.starts_with(&[0x1f, 0x8b]));
        let (_, decoded) = read_nbt(&raw).unwrap();
        assert_eq!(entries(&decoded).unwrap().len(), 1);
        assert_eq!(
            server_view(&entries(&decoded).unwrap()[0])["address"],
            "mc.hypixel.net"
        );

        normalize_format(&path, true).unwrap();
        assert!(fs::read(&path).unwrap().starts_with(&[0x1f, 0x8b]));
        let _ = fs::remove_file(path);
    }
    #[test]
    fn parses_java_server_addresses() {
        assert_eq!(
            parse_address("play.example.net").unwrap(),
            ("play.example.net".into(), 25565)
        );
        assert_eq!(
            parse_address("play.example.net:25570").unwrap(),
            ("play.example.net".into(), 25570)
        );
        assert_eq!(parse_address("[::1]:25565").unwrap(), ("::1".into(), 25565));
        assert!(parse_address("host:70000").is_err());
    }
    #[test]
    fn motd_flattens_json_styles_and_legacy_codes() {
        let lines = motd_segments(
            &json!({"text":"Welcome §cred","color":"gold","extra":[{"text":"!\nNext","bold":true}]}),
        );
        assert!(lines.iter().any(|line| line["text"] == "Welcome "));
        assert!(lines
            .iter()
            .any(|line| line["text"] == "red" && line["color"] == "#ff5555"));
        assert!(lines
            .iter()
            .any(|line| line["bold"] == true && line["text"].as_str().unwrap().contains("Next")));
    }
}
