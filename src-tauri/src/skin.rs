//! Mascotes (skins): o formato de pacote `.mascote`, a validação e a instalação.
//!
//! Um `.mascote` é um zip só com dados, nunca código. Nomes de arquivo permitidos:
//!
//! ```text
//! manifest.json        obrigatório: id, nome, autor, versão, licença, quadros por estado
//! sprites.png          obrigatório: spritesheet com quadros do mesmo tamanho, em grade
//! prop.png             obrigatório: onde o mascote se segura (o galho); serve para arrastar
//! preview.png          opcional: imagem de vitrine
//! sounds/done.*        opcional: som de "terminei" (.wav, .ogg ou .mp3)
//! sounds/waiting.*     opcional: som de "preciso de você"
//! ```
//!
//! Qualquer outro arquivo recusa o pacote inteiro. A preguiça padrão vem embutida no
//! executável e nunca pode ser sobrescrita. O formato está descrito em `docs/criar-mascote.md`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Versão do formato que este app entende. Pacotes mais novos pedem atualizar o app.
pub const FORMAT_VERSION: u32 = 1;
/// Id da preguiça embutida.
pub const DEFAULT_ID: &str = "preguica";

const MAX_PACKAGE: u64 = 2 * 1024 * 1024; // tamanho do .mascote
const MAX_UNPACKED: u64 = 4 * 1024 * 1024; // soma dos arquivos descompactados
const MAX_SOUND: usize = 512 * 1024;
const MAX_FRAME: u32 = 64; // lado máximo de um quadro, em pixels
const MAX_SHEET: u32 = 1024;
const MAX_PROP_W: u32 = 128;
const MAX_PROP_H: u32 = 32;
const MAX_ANIM_FRAMES: usize = 32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub frame: Size,
    pub states: States,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct States {
    pub idle: Anim,
    pub working: Anim,
    pub done: Anim,
    pub waiting: Anim,
}

/// Animação de um estado: índices de quadros na spritesheet (da esquerda para a
/// direita, de cima para baixo), quadros por segundo e um quadro de piscada opcional.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anim {
    pub frames: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blink: Option<u32>,
}

/// Mascote pronto para o frontend: manifest e arquivos como data URLs.
#[derive(Clone, Debug, Serialize)]
pub struct Skin {
    pub manifest: Manifest,
    pub sprites: String,
    pub prop: String,
    pub sounds: BTreeMap<String, String>,
}

impl Skin {
    /// Tamanho do conjunto (prop em cima, quadro embaixo), em pixels do sprite.
    pub fn content(&self) -> Size {
        let prop = png_size(&decode(&self.prop)).unwrap_or(Size { width: 0, height: 0 });
        Size {
            width: self.manifest.frame.width.max(prop.width),
            height: self.manifest.frame.height + prop.height,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Error {
    TooBig,
    NotZip,
    Forbidden(String),
    Missing(&'static str),
    Manifest(String),
    Newer(u32),
    Invalid(String),
    Io(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::TooBig => write!(f, "Esse mascote é grande demais (máximo de 2 MB)."),
            Error::NotZip => write!(f, "Esse arquivo não é um .mascote válido."),
            Error::Forbidden(name) => write!(f, "O pacote tem um arquivo não permitido: {name}"),
            Error::Missing(name) => write!(f, "Falta o arquivo {name} no pacote."),
            Error::Manifest(e) => write!(f, "O manifest.json está com erro: {e}"),
            Error::Newer(v) => write!(
                f,
                "Esse mascote usa o formato {v}, mais novo que o meu. Atualize o LENKO e tente de novo."
            ),
            Error::Invalid(e) => write!(f, "Mascote inválido: {e}"),
            Error::Io(e) => write!(f, "Não consegui instalar o mascote: {e}"),
        }
    }
}

fn invalid(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

// ---------------------------------------------------------------------------
// Validação (funções puras)
// ---------------------------------------------------------------------------

/// Arquivos aceitos dentro do pacote.
fn allowed(name: &str) -> bool {
    matches!(name, "manifest.json" | "sprites.png" | "prop.png" | "preview.png")
        || sound_kind(name).is_some()
}

/// `sounds/done.wav` → `Some("done")`.
fn sound_kind(name: &str) -> Option<&'static str> {
    let rest = name.strip_prefix("sounds/")?;
    let (stem, ext) = rest.split_once('.')?;
    if !matches!(ext, "wav" | "ogg" | "mp3") {
        return None;
    }
    ["done", "waiting"].into_iter().find(|k| *k == stem)
}

fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    (3..=40).contains(&b.len())
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
        && b[0] != b'-'
        && b[b.len() - 1] != b'-'
}

fn text(field: &str, value: &str, max: usize) -> Result<(), Error> {
    let len = value.trim().chars().count();
    if len == 0 || len > max {
        return Err(invalid(format!("o campo \"{field}\" deve ter de 1 a {max} caracteres")));
    }
    Ok(())
}

/// Lê largura e altura do cabeçalho de um PNG, sem decodificar a imagem.
pub fn png_size(bytes: &[u8]) -> Option<Size> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.len() < 24 || bytes[..8] != SIG || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let be = |i: usize| u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    Some(Size { width: be(16), height: be(20) })
}

fn sound_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        Some("audio/wav")
    } else if bytes.starts_with(b"OggS") {
        Some("audio/ogg")
    } else if bytes.starts_with(b"ID3") || (bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0) {
        Some("audio/mpeg")
    } else {
        None
    }
}

/// Confere o manifest e as imagens. `files` são os arquivos do pacote, já filtrados.
pub fn validate(files: &BTreeMap<String, Vec<u8>>) -> Result<Manifest, Error> {
    let raw = files.get("manifest.json").ok_or(Error::Missing("manifest.json"))?;
    let value: serde_json::Value =
        serde_json::from_slice(raw).map_err(|e| Error::Manifest(e.to_string()))?;
    // Checa a versão antes do resto: um formato novo pode ter mudado os campos
    if let Some(v) = value.get("formatVersion").and_then(|v| v.as_u64()) {
        if v > u64::from(FORMAT_VERSION) {
            return Err(Error::Newer(v as u32));
        }
    }
    let m: Manifest = serde_json::from_value(value).map_err(|e| Error::Manifest(e.to_string()))?;
    if m.format_version == 0 {
        return Err(Error::Manifest("formatVersion deve ser 1 ou maior".into()));
    }

    if !valid_id(&m.id) {
        return Err(invalid("o id deve ter de 3 a 40 letras minúsculas, números ou hífens"));
    }
    text("name", &m.name, 40)?;
    text("author", &m.author, 60)?;
    text("version", &m.version, 20)?;
    text("license", &m.license, 40)?;
    if let Some(d) = &m.description {
        text("description", d, 200)?;
    }

    let Size { width: fw, height: fh } = m.frame;
    if !(8..=MAX_FRAME).contains(&fw) || !(8..=MAX_FRAME).contains(&fh) {
        return Err(invalid(format!("o quadro deve ter de 8 a {MAX_FRAME} pixels de lado")));
    }

    let sheet = files.get("sprites.png").ok_or(Error::Missing("sprites.png"))?;
    let s = png_size(sheet).ok_or_else(|| invalid("sprites.png não é um PNG"))?;
    if s.width > MAX_SHEET || s.height > MAX_SHEET || s.width % fw != 0 || s.height % fh != 0 || s.width == 0 {
        return Err(invalid(format!(
            "sprites.png ({}×{}) deve ser uma grade de quadros {fw}×{fh}, com até {MAX_SHEET} pixels de lado",
            s.width, s.height
        )));
    }
    let count = (s.width / fw) * (s.height / fh);

    let prop = files.get("prop.png").ok_or(Error::Missing("prop.png"))?;
    let p = png_size(prop).ok_or_else(|| invalid("prop.png não é um PNG"))?;
    if p.width == 0 || p.height == 0 || p.width > MAX_PROP_W || p.height > MAX_PROP_H {
        return Err(invalid(format!("prop.png deve ter até {MAX_PROP_W}×{MAX_PROP_H} pixels")));
    }
    if let Some(preview) = files.get("preview.png") {
        png_size(preview).ok_or_else(|| invalid("preview.png não é um PNG"))?;
    }

    let st = &m.states;
    for (name, anim) in [("idle", &st.idle), ("working", &st.working), ("done", &st.done), ("waiting", &st.waiting)] {
        if anim.frames.is_empty() || anim.frames.len() > MAX_ANIM_FRAMES {
            return Err(invalid(format!("o estado \"{name}\" deve ter de 1 a {MAX_ANIM_FRAMES} quadros")));
        }
        if anim.frames.iter().chain(anim.blink.iter()).any(|&i| i >= count) {
            return Err(invalid(format!("o estado \"{name}\" usa um quadro que não existe (há {count})")));
        }
        if anim.fps.is_some_and(|f| !(1..=24).contains(&f)) {
            return Err(invalid(format!("o fps do estado \"{name}\" deve ser de 1 a 24")));
        }
    }

    for (name, bytes) in files {
        if sound_kind(name).is_some() && (bytes.len() > MAX_SOUND || sound_mime(bytes).is_none()) {
            return Err(invalid(format!("{name} não é um som válido de até 512 KB")));
        }
    }
    Ok(m)
}

/// Lê o zip aplicando a lista de arquivos permitidos e o limite de tamanho real
/// (o tamanho declarado no zip não é confiável).
pub fn unpack(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    if bytes.len() as u64 > MAX_PACKAGE {
        return Err(Error::TooBig);
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| Error::NotZip)?;
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|_| Error::NotZip)?;
        let name = entry.name().map_err(|_| Error::NotZip)?.replace('\\', "/");
        if entry.is_dir() {
            if name.trim_end_matches('/') == "sounds" {
                continue;
            }
            return Err(Error::Forbidden(name));
        }
        if !allowed(&name) || files.contains_key(&name) {
            return Err(Error::Forbidden(name));
        }
        let mut data = Vec::new();
        (&mut entry)
            .take(MAX_UNPACKED - total + 1)
            .read_to_end(&mut data)
            .map_err(|_| Error::NotZip)?;
        total += data.len() as u64;
        if total > MAX_UNPACKED {
            return Err(Error::TooBig);
        }
        files.insert(name, data);
    }
    Ok(files)
}

// ---------------------------------------------------------------------------
// Pasta de mascotes instalados: `app_data_dir/mascotes/<id>/`
// ---------------------------------------------------------------------------

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("mascotes")
}

/// Instala (ou atualiza) um `.mascote`. Grava numa pasta temporária e só troca no fim,
/// para nunca deixar um mascote pela metade.
pub fn install(data_dir: &Path, package: &Path) -> Result<Manifest, Error> {
    let io = |e: std::io::Error| Error::Io(e.to_string());
    let size = fs::metadata(package).map_err(io)?.len();
    if size > MAX_PACKAGE {
        return Err(Error::TooBig);
    }
    let files = unpack(&fs::read(package).map_err(io)?)?;
    let manifest = validate(&files)?;
    if manifest.id == DEFAULT_ID {
        return Err(invalid("o id \"preguica\" é reservado para o mascote padrão"));
    }

    let root = dir(data_dir);
    let tmp = root.join(format!(".{}.tmp", manifest.id));
    let final_dir = root.join(&manifest.id);
    let _ = fs::remove_dir_all(&tmp);
    for (name, data) in &files {
        let path = tmp.join(name);
        fs::create_dir_all(path.parent().expect("sempre dentro de tmp")).map_err(io)?;
        fs::write(path, data).map_err(io)?;
    }
    let _ = fs::remove_dir_all(&final_dir);
    fs::rename(&tmp, &final_dir).map_err(io)?;
    Ok(manifest)
}

/// Remove um mascote instalado. A preguiça padrão não pode ser removida.
pub fn remove(data_dir: &Path, id: &str) -> Result<(), Error> {
    if id == DEFAULT_ID || !valid_id(id) {
        return Err(invalid("esse mascote não pode ser removido"));
    }
    fs::remove_dir_all(dir(data_dir).join(id)).map_err(|e| Error::Io(e.to_string()))
}

/// Lê os arquivos de um mascote instalado (com a mesma lista de permitidos).
fn read_installed(data_dir: &Path, id: &str) -> Option<BTreeMap<String, Vec<u8>>> {
    if !valid_id(id) {
        return None;
    }
    let base = dir(data_dir).join(id);
    let mut files = BTreeMap::new();
    for name in ["manifest.json", "sprites.png", "prop.png", "preview.png"] {
        if let Ok(data) = fs::read(base.join(name)) {
            files.insert(name.to_string(), data);
        }
    }
    for entry in fs::read_dir(base.join("sounds")).into_iter().flatten().flatten() {
        let name = format!("sounds/{}", entry.file_name().to_string_lossy());
        if sound_kind(&name).is_some() {
            files.insert(name, fs::read(entry.path()).ok()?);
        }
    }
    Some(files)
}

/// Mascotes disponíveis: a preguiça primeiro, depois os instalados em ordem de nome.
/// Pastas quebradas ou editadas à mão para algo inválido ficam de fora.
pub fn list(data_dir: &Path) -> Vec<Manifest> {
    let mut found: Vec<Manifest> = fs::read_dir(dir(data_dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| read_installed(data_dir, &e.file_name().to_string_lossy()))
        .filter_map(|files| validate(&files).ok())
        .filter(|m| m.id != DEFAULT_ID)
        .collect();
    found.sort_by_key(|m| m.name.to_lowercase());
    let mut all = vec![default_manifest()];
    all.extend(found);
    all
}

// ---------------------------------------------------------------------------
// Preguiça embutida e montagem para o frontend
// ---------------------------------------------------------------------------

fn default_files() -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("manifest.json".into(), include_bytes!("../../src/mascotes/preguica/manifest.json").to_vec()),
        ("sprites.png".into(), include_bytes!("../../src/mascotes/preguica/sprites.png").to_vec()),
        ("prop.png".into(), include_bytes!("../../src/mascotes/preguica/prop.png").to_vec()),
    ])
}

pub fn default_manifest() -> Manifest {
    validate(&default_files()).expect("a preguiça embutida é válida (ver testes)")
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    use base64::Engine;
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn decode(url: &str) -> Vec<u8> {
    use base64::Engine;
    url.split_once(',')
        .and_then(|(_, b64)| base64::engine::general_purpose::STANDARD.decode(b64).ok())
        .unwrap_or_default()
}

fn build(files: &BTreeMap<String, Vec<u8>>) -> Result<Skin, Error> {
    let manifest = validate(files)?;
    let sounds = files
        .iter()
        .filter_map(|(name, bytes)| {
            let kind = sound_kind(name)?;
            Some((kind.to_string(), data_url(sound_mime(bytes)?, bytes)))
        })
        .collect();
    Ok(Skin {
        manifest,
        sprites: data_url("image/png", &files["sprites.png"]),
        prop: data_url("image/png", &files["prop.png"]),
        sounds,
    })
}

/// Carrega o mascote pedido; se ele sumiu ou quebrou, volta para a preguiça.
pub fn load(data_dir: &Path, id: Option<&str>) -> Skin {
    id.filter(|id| *id != DEFAULT_ID)
        .and_then(|id| read_installed(data_dir, id))
        .and_then(|files| build(&files).ok())
        .unwrap_or_else(|| build(&default_files()).expect("a preguiça embutida é válida"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            out.start_file(*name, opts).unwrap();
            out.write_all(data).unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    /// Pacote válido baseado na preguiça, com outro id.
    fn sample(id: &str) -> BTreeMap<String, Vec<u8>> {
        let mut files = default_files();
        let mut m: serde_json::Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
        m["id"] = id.into();
        m["name"] = "Coruja".into();
        files.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
        files
    }

    fn with_manifest(edit: impl Fn(&mut serde_json::Value)) -> Result<Manifest, Error> {
        let mut files = sample("coruja");
        let mut m: serde_json::Value = serde_json::from_slice(&files["manifest.json"]).unwrap();
        edit(&mut m);
        files.insert("manifest.json".into(), serde_json::to_vec(&m).unwrap());
        validate(&files)
    }

    #[test]
    fn preguica_embutida_e_valida() {
        let m = default_manifest();
        assert_eq!(m.id, DEFAULT_ID);
        let skin = load(Path::new("pasta-que-nao-existe"), None);
        assert_eq!(skin.content(), Size { width: 32, height: 27 });
    }

    #[test]
    fn recusa_arquivos_fora_da_lista() {
        for bad in ["index.html", "script.js", "../fora.png", "sounds/done.exe", "sounds/outro.wav", "a/sprites.png"] {
            let pkg = zip(&[("manifest.json", b"{}"), (bad, b"x")]);
            assert_eq!(unpack(&pkg), Err(Error::Forbidden(bad.into())), "{bad}");
        }
    }

    #[test]
    fn recusa_pacote_grande_ou_que_nao_e_zip() {
        assert_eq!(unpack(b"nao sou um zip"), Err(Error::NotZip));
        assert_eq!(unpack(&vec![0; MAX_PACKAGE as usize + 1]), Err(Error::TooBig));
        // Bomba de compressão: pequeno compactado, enorme aberto
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        out.start_file("sprites.png", opts).unwrap();
        out.write_all(&vec![0u8; MAX_UNPACKED as usize + 10]).unwrap();
        let bomb = out.finish().unwrap().into_inner();
        assert!(bomb.len() < MAX_PACKAGE as usize);
        assert_eq!(unpack(&bomb), Err(Error::TooBig));
    }

    #[test]
    fn aceita_pacote_valido_com_som() {
        let mut files = sample("coruja");
        files.insert("sounds/done.wav".into(), b"RIFF\0\0\0\0WAVEfmt ".to_vec());
        let entries: Vec<(&str, &[u8])> = files.iter().map(|(k, v)| (k.as_str(), v.as_slice())).collect();
        let unpacked = unpack(&zip(&entries)).unwrap();
        assert_eq!(validate(&unpacked).unwrap().id, "coruja");
        let skin = build(&unpacked).unwrap();
        assert!(skin.sounds["done"].starts_with("data:audio/wav;base64,"));
    }

    #[test]
    fn valida_o_manifest() {
        assert_eq!(with_manifest(|m| m["formatVersion"] = 2.into()), Err(Error::Newer(2)));
        assert!(matches!(with_manifest(|m| m["id"] = "Com Espaço".into()), Err(Error::Invalid(_))));
        assert!(matches!(with_manifest(|m| m["name"] = "".into()), Err(Error::Invalid(_))));
        assert!(matches!(with_manifest(|m| m["frame"]["width"] = 33.into()), Err(Error::Invalid(_))));
        assert!(matches!(with_manifest(|m| m["states"]["done"]["frames"] = serde_json::json!([99])), Err(Error::Invalid(_))));
        assert!(matches!(with_manifest(|m| m["states"]["done"]["frames"] = serde_json::json!([])), Err(Error::Invalid(_))));
        assert!(matches!(with_manifest(|m| m["states"]["idle"]["fps"] = 0.into()), Err(Error::Invalid(_))));
        assert!(matches!(with_manifest(|m| { m["states"].as_object_mut().unwrap().remove("waiting"); }), Err(Error::Manifest(_))));
        // Campos desconhecidos são ignorados (versões menores do formato podem acrescentar)
        assert!(with_manifest(|m| m["novidade"] = true.into()).is_ok());
    }

    #[test]
    fn recusa_imagens_e_sons_invalidos() {
        let mut files = sample("coruja");
        files.insert("sprites.png".into(), b"nao sou png".to_vec());
        assert!(matches!(validate(&files), Err(Error::Invalid(_))));

        let mut files = sample("coruja");
        files.remove("prop.png");
        assert_eq!(validate(&files), Err(Error::Missing("prop.png")));

        let mut files = sample("coruja");
        files.insert("sounds/waiting.mp3".into(), b"<script>".to_vec());
        assert!(matches!(validate(&files), Err(Error::Invalid(_))));
    }

    #[test]
    fn instala_lista_carrega_e_remove() {
        let data = std::env::temp_dir().join(format!("lenko-mascotes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data);
        fs::create_dir_all(&data).unwrap();

        let files = sample("coruja");
        let entries: Vec<(&str, &[u8])> = files.iter().map(|(k, v)| (k.as_str(), v.as_slice())).collect();
        let pkg = data.join("coruja.mascote");
        fs::write(&pkg, zip(&entries)).unwrap();

        assert_eq!(install(&data, &pkg).unwrap().id, "coruja");
        install(&data, &pkg).unwrap(); // reinstalar substitui sem erro
        let ids: Vec<String> = list(&data).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, ["preguica", "coruja"]);
        assert_eq!(load(&data, Some("coruja")).manifest.name, "Coruja");

        // Não deixa sobrescrever a preguiça
        let files = sample(DEFAULT_ID);
        let entries: Vec<(&str, &[u8])> = files.iter().map(|(k, v)| (k.as_str(), v.as_slice())).collect();
        fs::write(&pkg, zip(&entries)).unwrap();
        assert!(matches!(install(&data, &pkg), Err(Error::Invalid(_))));

        remove(&data, "coruja").unwrap();
        assert_eq!(load(&data, Some("coruja")).manifest.id, DEFAULT_ID); // sumiu → preguiça
        assert!(remove(&data, DEFAULT_ID).is_err());
        assert!(remove(&data, "../fora").is_err());
        fs::remove_dir_all(&data).unwrap();
    }
}
