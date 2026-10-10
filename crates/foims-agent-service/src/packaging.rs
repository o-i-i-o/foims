//! 按操作系统/架构动态组包（zip / deb / rpm），设计来源 docs/agent-design.md §6.3。
//!
//! 全部在内存中完成，不落临时文件：
//! - zip：二进制 + agent.toml + ca.pem + install.sh + README（通用，含 sysvinit 兜底）；
//! - deb：纯 Rust 手写（tar + flate2 gzip + ar 归档），含 systemd unit 与 postinst；
//! - rpm：`rpm` crate（免 rpmbuild、不签名），内容与 deb 等价，agent.toml 标记 config。

use std::io::Write as _;

use flate2::write::GzEncoder;

/// 组包统一输入（二进制已由 manifest 校验）。
#[derive(Clone)]
pub struct PackageInputs<'a> {
    /// 已校验的 agent 二进制
    pub binary: &'a [u8],
    /// rust target triple（如 x86_64-unknown-linux-musl）
    pub target: &'a str,
    /// agent 版本（取自 manifest）
    pub agent_version: &'a str,
    /// 完整 agent.toml 文本（含 server_addr/token）
    pub agent_toml: &'a str,
    /// 站点 CA 公钥
    pub ca_pem: &'a [u8],
    /// Agent 客户端证书 PEM（mTLS 上报身份）
    pub client_cert_pem: &'a str,
    /// Agent 客户端私钥 PEM（0600 语义落位）
    pub client_key_pem: &'a str,
    /// install.sh 文本（zip 场景使用）
    pub install_sh: &'a str,
}

/// systemd unit 内容（deb / rpm 共用）。
pub const SYSTEMD_UNIT: &str = r#"[Unit]
Description=FOIMS Agent host metrics collector
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/foims-agent --config /etc/foims-agent/agent.toml
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
"#;

/// deb postinst（rpm post scriptlet 语义相同）。
pub const DEB_POSTINST: &str = r#"#!/bin/sh
set -e
if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload || true
    if command -v deb-systemd-invoke >/dev/null 2>&1; then
        deb-systemd-invoke enable --now foims-agent.service || true
    else
        systemctl enable --now foims-agent.service || true
    fi
fi
"#;

/// zip 包内 README.txt（简短安装说明）。
pub const ZIP_README: &str = r#"FOIMS Agent 安装包
==================

1. 解压本压缩包后，以 root 执行安装：
   sudo bash install.sh
   （脚本会把 foims-agent 安装到 /usr/local/bin，配置与 mTLS 客户端证书写入
   /etc/foims-agent/，并在有 systemd 的系统上自动 enable --now foims-agent.service）

2. 配置文件位置：/etc/foims-agent/agent.toml（含上报地址 server_addr 与本机 token，
   token 与 client.key 属敏感物料，请勿泄露）。

3. 服务状态查看：systemctl status foims-agent

4. 卸载方法：
   sudo systemctl disable --now foims-agent.service 2>/dev/null
   sudo rm -f /usr/local/bin/foims-agent /usr/lib/systemd/system/foims-agent.service
   sudo rm -rf /etc/foims-agent
"#;

/// target triple → 通用架构标签（未知 target 返回 None）。
///
/// 仅取 triple 首段判断：同一首段的 musl/gnu 变体产物架构一致。
pub fn arch_label(target: &str) -> Option<&'static str> {
    let first = target.split('-').next()?;
    match first {
        "x86_64" => Some("x86_64"),
        "aarch64" => Some("aarch64"),
        "i686" => Some("i686"),
        "armv7" => Some("armv7"),
        "arm" => Some("arm"),
        _ => None,
    }
}

/// 通用架构标签 → Debian 架构名（未知架构返回 None）。
pub fn deb_arch(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" => Some("amd64"),
        "aarch64" => Some("arm64"),
        "i686" => Some("i386"),
        "armv7" | "arm" => Some("armhf"),
        _ => None,
    }
}

/// 通用架构标签 → RPM 架构名（未知架构返回 None）。
pub fn rpm_arch(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" => Some("x86_64"),
        "aarch64" => Some("aarch64"),
        "i686" => Some("i686"),
        "armv7" => Some("armv7hl"),
        "arm" => Some("armhfp"),
        _ => None,
    }
}

/// 组装 zip 安装包：foims-agent(0755) / agent.toml(0600) / ca.pem(0644) /
/// client.pem(0644) / client.key(0600) / install.sh(0755) / README.txt(0644)，
/// 文件名不带目录前缀。
pub fn build_zip(inputs: PackageInputs<'_>) -> Result<Vec<u8>, String> {
    fn start(
        writer: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>,
        name: &str,
        mode: u32,
    ) -> Result<(), String> {
        let options = zip::write::SimpleFileOptions::default().unix_permissions(mode);
        writer
            .start_file(name, options)
            .map_err(|e| format!("zip 写入条目 {name} 失败: {e}"))
    }

    let buf = Vec::new();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(buf));

    start(&mut writer, "foims-agent", 0o755)?;
    writer
        .write_all(inputs.binary)
        .map_err(|e| format!("zip 写入 foims-agent 内容失败: {e}"))?;

    start(&mut writer, "agent.toml", 0o600)?;
    writer
        .write_all(inputs.agent_toml.as_bytes())
        .map_err(|e| format!("zip 写入 agent.toml 内容失败: {e}"))?;

    start(&mut writer, "ca.pem", 0o644)?;
    writer
        .write_all(inputs.ca_pem)
        .map_err(|e| format!("zip 写入 ca.pem 内容失败: {e}"))?;

    start(&mut writer, "client.pem", 0o644)?;
    writer
        .write_all(inputs.client_cert_pem.as_bytes())
        .map_err(|e| format!("zip 写入 client.pem 内容失败: {e}"))?;

    start(&mut writer, "client.key", 0o600)?;
    writer
        .write_all(inputs.client_key_pem.as_bytes())
        .map_err(|e| format!("zip 写入 client.key 内容失败: {e}"))?;

    start(&mut writer, "install.sh", 0o755)?;
    writer
        .write_all(inputs.install_sh.as_bytes())
        .map_err(|e| format!("zip 写入 install.sh 内容失败: {e}"))?;

    start(&mut writer, "README.txt", 0o644)?;
    writer
        .write_all(ZIP_README.as_bytes())
        .map_err(|e| format!("zip 写入 README.txt 内容失败: {e}"))?;

    writer
        .finish()
        .map_err(|e| format!("zip 收尾失败: {e}"))
        .map(std::io::Cursor::into_inner)
}

/// deb 包控制文件 control 文本。
fn deb_control(agent_version: &str, arch: &str) -> String {
    format!(
        "Package: foims-agent\n\
         Version: {agent_version}\n\
         Section: net\n\
         Priority: optional\n\
         Architecture: {arch}\n\
         Maintainer: oi-io <boss@oi-io.cc>\n\
         Description: FOIMS Agent host metrics collector\n\
         \x20Node-style host metrics agent reporting to FOIMS over HTTP/3.\n"
    )
}

/// 向 gzip 压缩的 tar 流中追加单个文件条目（mtime/uid/gid 固定为 0）。
fn append_tar_entry(
    builder: &mut tar::Builder<GzEncoder<Vec<u8>>>,
    name: &str,
    data: &[u8],
    mode: u32,
) -> Result<(), String> {
    let mut header = tar::Header::new_gnu();
    header.set_mode(mode);
    header.set_mtime(0);
    header.set_uid(0);
    header.set_gid(0);
    header.set_size(data.len() as u64);
    builder
        .append_data(&mut header, name, data)
        .map_err(|e| format!("tar 写入条目 {name} 失败: {e}"))
}

/// 构造一段 gzip 压缩的 tar 数据流。
fn gzip_tar(
    build: impl FnOnce(&mut tar::Builder<GzEncoder<Vec<u8>>>) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    let encoder = GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    build(&mut builder)?;
    let encoder = builder
        .into_inner()
        .map_err(|e| format!("tar 收尾失败: {e}"))?;
    encoder.finish().map_err(|e| format!("gzip 收尾失败: {e}"))
}

/// 手写 ar 成员头（60 字节）：name 补空格至 16 并以 `/` 结尾、mtime=0（12 字节）、
/// uid/gid=0（各 6 字节）、mode=100644（8 字节）、size（10 字节）、magic "`\n"；
/// 数据按 2 字节对齐，奇数长度补 `\n`。
fn append_ar_member(out: &mut Vec<u8>, name: &str, data: &[u8]) -> Result<(), String> {
    if name.len() + 1 > 16 {
        return Err(format!("ar 成员名过长: {name}"));
    }
    let mut header = String::with_capacity(60);
    header.push_str(&format!("{:<16}", format!("{name}/")));
    header.push_str(&format!("{:<12}", "0")); // mtime
    header.push_str(&format!("{:<6}", "0")); // uid
    header.push_str(&format!("{:<6}", "0")); // gid
    header.push_str(&format!("{:<8}", "100644")); // mode
    header.push_str(&format!("{:<10}", data.len())); // size
    header.push('\u{60}');
    header.push('\n');
    if header.len() != 60 {
        return Err(format!("ar 成员头长度异常: {name}"));
    }
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(b'\n');
    }
    Ok(())
}

/// 组装 deb 安装包（纯 Rust，不调用外部命令）。
///
/// data.tar.gz：二进制(0755) + agent.toml(0600) + ca.pem(0644) +
/// client.pem(0644) + client.key(0600) + systemd unit(0644)；
/// control.tar.gz：control(0644) + postinst(0755)；
/// ar 成员依次：debian-binary、control.tar.gz、data.tar.gz。
pub fn build_deb(inputs: PackageInputs<'_>) -> Result<Vec<u8>, String> {
    let arch = arch_label(inputs.target)
        .ok_or_else(|| format!("未知 target: {}", inputs.target))
        .and_then(|a| {
            deb_arch(a).ok_or_else(|| format!("target {} 无对应 deb 架构", inputs.target))
        })?;

    let data_gz = gzip_tar(|builder| {
        append_tar_entry(builder, "./usr/local/bin/foims-agent", inputs.binary, 0o755)?;
        append_tar_entry(
            builder,
            "./etc/foims-agent/agent.toml",
            inputs.agent_toml.as_bytes(),
            0o600,
        )?;
        append_tar_entry(builder, "./etc/foims-agent/ca.pem", inputs.ca_pem, 0o644)?;
        append_tar_entry(
            builder,
            "./etc/foims-agent/client.pem",
            inputs.client_cert_pem.as_bytes(),
            0o644,
        )?;
        append_tar_entry(
            builder,
            "./etc/foims-agent/client.key",
            inputs.client_key_pem.as_bytes(),
            0o600,
        )?;
        append_tar_entry(
            builder,
            "./usr/lib/systemd/system/foims-agent.service",
            SYSTEMD_UNIT.as_bytes(),
            0o644,
        )?;
        Ok(())
    })?;

    let control_gz = gzip_tar(|builder| {
        append_tar_entry(
            builder,
            "./control",
            deb_control(inputs.agent_version, arch).as_bytes(),
            0o644,
        )?;
        append_tar_entry(builder, "./postinst", DEB_POSTINST.as_bytes(), 0o755)?;
        Ok(())
    })?;

    let mut out = Vec::new();
    out.extend_from_slice(b"!<arch>\n");
    append_ar_member(&mut out, "debian-binary", b"2.0\n")?;
    append_ar_member(&mut out, "control.tar.gz", &control_gz)?;
    append_ar_member(&mut out, "data.tar.gz", &data_gz)?;
    Ok(out)
}

/// 组装 rpm 安装包（rpm crate，不签名）。
///
/// 安装文件与 deb 等价；agent.toml 标记 config；post scriptlet 与 deb postinst 同语义。
/// rpm crate 的 `with_file_contents` 直接接收内存内容，无需落临时文件。
pub fn build_rpm(inputs: PackageInputs<'_>) -> Result<Vec<u8>, String> {
    let arch = arch_label(inputs.target)
        .ok_or_else(|| format!("未知 target: {}", inputs.target))
        .and_then(|a| {
            rpm_arch(a).ok_or_else(|| format!("target {} 无对应 rpm 架构", inputs.target))
        })?;

    let mut builder = rpm::PackageBuilder::new(
        "foims-agent",
        inputs.agent_version,
        "GPL-3.0-or-later",
        arch,
        "FOIMS Agent host metrics collector",
    );
    builder
        .with_file_contents(
            inputs.binary.to_vec(),
            rpm::FileOptions::new("/usr/local/bin/foims-agent").permissions(0o755),
        )
        .map_err(|e| format!("rpm 添加 foims-agent 失败: {e}"))?;
    builder
        .with_file_contents(
            inputs.agent_toml,
            rpm::FileOptions::new("/etc/foims-agent/agent.toml")
                .permissions(0o600)
                .config(),
        )
        .map_err(|e| format!("rpm 添加 agent.toml 失败: {e}"))?;
    builder
        .with_file_contents(
            inputs.ca_pem.to_vec(),
            rpm::FileOptions::new("/etc/foims-agent/ca.pem").permissions(0o644),
        )
        .map_err(|e| format!("rpm 添加 ca.pem 失败: {e}"))?;
    builder
        .with_file_contents(
            inputs.client_cert_pem,
            rpm::FileOptions::new("/etc/foims-agent/client.pem").permissions(0o644),
        )
        .map_err(|e| format!("rpm 添加 client.pem 失败: {e}"))?;
    builder
        .with_file_contents(
            inputs.client_key_pem,
            rpm::FileOptions::new("/etc/foims-agent/client.key").permissions(0o600),
        )
        .map_err(|e| format!("rpm 添加 client.key 失败: {e}"))?;
    builder
        .with_file_contents(
            SYSTEMD_UNIT,
            rpm::FileOptions::new("/usr/lib/systemd/system/foims-agent.service").permissions(0o644),
        )
        .map_err(|e| format!("rpm 添加 unit 失败: {e}"))?;
    builder.post_install_script(DEB_POSTINST);

    let package = builder.build().map_err(|e| format!("rpm 构建失败: {e}"))?;
    let mut out = std::io::Cursor::new(Vec::new());
    package
        .write(&mut out)
        .map_err(|e| format!("rpm 序列化失败: {e}"))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    const TARGET: &str = "x86_64-unknown-linux-musl";
    const VERSION: &str = "0.21.15";

    /// 组包测试输入
    fn test_inputs() -> (Vec<u8>, PackageInputs<'static>) {
        // 泄漏测试常量避免自引用生命周期问题（进程退出即回收）
        let binary: &'static [u8] =
            Box::leak(b"foims-agent-fake-binary".to_vec().into_boxed_slice());
        let toml: &'static str = Box::leak(
            format!("server_addr = \"10.0.0.1:9100\"\ntoken = \"tok\"\n").into_boxed_str(),
        );
        let ca: &'static [u8] = Box::leak(
            b"-----BEGIN CERTIFICATE-----\nCA\n-----END CERTIFICATE-----\n"
                .to_vec()
                .into_boxed_slice(),
        );
        let client_cert: &'static str = Box::leak(
            "-----BEGIN CERTIFICATE-----\nCLIENT\n-----END CERTIFICATE-----\n"
                .to_string()
                .into_boxed_str(),
        );
        let client_key: &'static str = Box::leak(
            "-----BEGIN PRIVATE KEY-----\nCLIENTKEY\n-----END PRIVATE KEY-----\n"
                .to_string()
                .into_boxed_str(),
        );
        let sh: &'static str = Box::leak(
            "#!/bin/sh\nset -e\ninstall -m 0755 foims-agent /usr/local/bin/foims-agent\n"
                .to_string()
                .into_boxed_str(),
        );
        let inputs = PackageInputs {
            binary,
            target: TARGET,
            agent_version: VERSION,
            agent_toml: toml,
            ca_pem: ca,
            client_cert_pem: client_cert,
            client_key_pem: client_key,
            install_sh: sh,
        };
        (binary.to_vec(), inputs)
    }

    /// 唯一临时目录
    fn temp_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("foims-agent-service-pkg-{tag}-{ts}-{n}"));
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("创建临时目录失败: {e}"));
        dir
    }

    /// 解析手写 ar 归档：返回 (成员名, 数据) 列表
    fn parse_ar(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
        if data.len() < 8 || &data[..8] != b"!<arch>\n" {
            return Err("ar 全局头不正确".to_string());
        }
        let mut offset = 8;
        let mut members = Vec::new();
        while offset < data.len() {
            let header_end = offset + 60;
            if data.len() < header_end {
                return Err("ar 成员头不完整".to_string());
            }
            let header = &data[offset..header_end];
            let name = String::from_utf8_lossy(&header[..16])
                .trim_end()
                .trim_end_matches('/')
                .to_string();
            let size_str = String::from_utf8_lossy(&header[48..58]);
            let size: usize = size_str
                .trim()
                .parse()
                .map_err(|e| format!("ar 成员 {name} 大小解析失败: {e}"))?;
            let data_start = header_end;
            let data_end = data_start.checked_add(size).ok_or("ar 成员大小溢出")?;
            if data.len() < data_end {
                return Err("ar 成员数据不完整".to_string());
            }
            members.push((name, data[data_start..data_end].to_vec()));
            // 数据按 2 字节对齐
            let padded = size + (size % 2);
            offset = data_start + padded;
        }
        Ok(members)
    }

    /// 解压 tar.gz 并返回 (路径, mode) 列表与 (路径, 内容) 列表
    fn parse_tar_gz(data: &[u8]) -> Result<Vec<(String, u32, Vec<u8>)>, String> {
        let decoder = flate2::read::GzDecoder::new(data);
        let mut archive = tar::Archive::new(decoder);
        let mut entries = Vec::new();
        for entry in archive
            .entries()
            .map_err(|e| format!("tar 遍历失败: {e}"))?
        {
            let mut entry = entry.map_err(|e| format!("tar 条目读取失败: {e}"))?;
            let path = entry
                .path()
                .map_err(|e| format!("tar 路径读取失败: {e}"))?
                .to_string_lossy()
                .into_owned();
            let mode = entry
                .header()
                .mode()
                .map_err(|e| format!("tar mode 读取失败: {e}"))?;
            let mut content = Vec::new();
            entry
                .read_to_end(&mut content)
                .map_err(|e| format!("tar 内容读取失败: {e}"))?;
            entries.push((path, mode, content));
        }
        Ok(entries)
    }

    #[test]
    fn arch_映射全覆盖() {
        assert_eq!(arch_label("x86_64-unknown-linux-musl"), Some("x86_64"));
        assert_eq!(arch_label("aarch64-unknown-linux-musl"), Some("aarch64"));
        assert_eq!(arch_label("i686-unknown-linux-musl"), Some("i686"));
        assert_eq!(arch_label("armv7-unknown-linux-musleabihf"), Some("armv7"));
        assert_eq!(arch_label("arm-unknown-linux-musleabihf"), Some("arm"));
        assert_eq!(arch_label("mips-unknown-linux-musl"), None);
        assert_eq!(arch_label(""), None);

        assert_eq!(deb_arch("x86_64"), Some("amd64"));
        assert_eq!(deb_arch("aarch64"), Some("arm64"));
        assert_eq!(deb_arch("i686"), Some("i386"));
        assert_eq!(deb_arch("armv7"), Some("armhf"));
        assert_eq!(deb_arch("arm"), Some("armhf"));
        assert_eq!(deb_arch("riscv64"), None);

        assert_eq!(rpm_arch("x86_64"), Some("x86_64"));
        assert_eq!(rpm_arch("aarch64"), Some("aarch64"));
        assert_eq!(rpm_arch("i686"), Some("i686"));
        assert_eq!(rpm_arch("armv7"), Some("armv7hl"));
        assert_eq!(rpm_arch("arm"), Some("armhfp"));
        assert_eq!(rpm_arch("s390x"), None);
    }

    #[test]
    fn zip_读回验证条目与权限() {
        let (_, inputs) = test_inputs();
        let data = build_zip(inputs).unwrap_or_else(|e| panic!("zip 组包失败: {e}"));

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data))
            .unwrap_or_else(|e| panic!("zip 读回失败: {e}"));
        let expected = [
            ("foims-agent", 0o755),
            ("agent.toml", 0o600),
            ("ca.pem", 0o644),
            ("client.pem", 0o644),
            ("client.key", 0o600),
            ("install.sh", 0o755),
            ("README.txt", 0o644),
        ];
        assert_eq!(archive.len(), expected.len(), "zip 条目数应一致");
        for (name, mode) in expected {
            let mut file = archive
                .by_name(name)
                .unwrap_or_else(|e| panic!("zip 缺少条目 {name}: {e}"));
            // unix_mode 含文件类型位（0o100000），取低 12 位比较
            assert_eq!(
                file.unix_mode().map(|m| m & 0o7777),
                Some(mode),
                "条目 {name} 权限应为 {mode:o}"
            );
            let mut content = Vec::new();
            file.read_to_end(&mut content)
                .unwrap_or_else(|e| panic!("zip 条目 {name} 读取失败: {e}"));
            assert!(!content.is_empty(), "条目 {name} 内容不应为空");
        }
    }

    #[test]
    fn deb_结构成员与tar内容验证() {
        let (binary, inputs) = test_inputs();
        let data = build_deb(inputs).unwrap_or_else(|e| panic!("deb 组包失败: {e}"));

        let members = parse_ar(&data).unwrap_or_else(|e| panic!("ar 解析失败: {e}"));
        let names: Vec<&str> = members.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["debian-binary", "control.tar.gz", "data.tar.gz"]);
        assert_eq!(members[0].1, b"2.0\n", "debian-binary 内容应为 2.0");

        // control.tar.gz：control 与 postinst（tar crate 写入时归一掉 "./" 前缀，
        // dpkg 对 "control" 与 "./control" 均接受）
        let control_entries =
            parse_tar_gz(&members[1].1).unwrap_or_else(|e| panic!("control.tar.gz 解析失败: {e}"));
        assert_eq!(control_entries.len(), 2, "control.tar.gz 应有 2 个条目");
        assert_eq!(control_entries[0].0, "control");
        assert_eq!(control_entries[0].1, 0o644);
        let control_text = String::from_utf8_lossy(&control_entries[0].2);
        assert!(control_text.contains("Package: foims-agent"));
        assert!(control_text.contains(&format!("Version: {VERSION}")));
        assert!(control_text.contains("Architecture: amd64"));
        assert!(control_text.contains("Maintainer: oi-io <boss@oi-io.cc>"));
        assert_eq!(control_entries[1].0, "postinst");
        assert_eq!(control_entries[1].1, 0o755);
        assert!(
            String::from_utf8_lossy(&control_entries[1].2)
                .contains("systemctl enable --now foims-agent.service")
        );

        // data.tar.gz：安装文件与权限
        let data_entries =
            parse_tar_gz(&members[2].1).unwrap_or_else(|e| panic!("data.tar.gz 解析失败: {e}"));
        let expect_files = [
            ("usr/local/bin/foims-agent", 0o755),
            ("etc/foims-agent/agent.toml", 0o600),
            ("etc/foims-agent/ca.pem", 0o644),
            ("etc/foims-agent/client.pem", 0o644),
            ("etc/foims-agent/client.key", 0o600),
            ("usr/lib/systemd/system/foims-agent.service", 0o644),
        ];
        assert_eq!(
            data_entries.len(),
            expect_files.len(),
            "data.tar.gz 条目数应一致"
        );
        for (idx, (path, mode)) in expect_files.iter().enumerate() {
            assert_eq!(&data_entries[idx].0, path);
            assert_eq!(&data_entries[idx].1, mode, "条目 {path} 权限应一致");
        }
        assert_eq!(data_entries[0].2, binary, "二进制内容应一致");
        assert_eq!(data_entries[5].2, SYSTEMD_UNIT.as_bytes());
    }

    #[test]
    fn rpm_魔数与元数据读回验证() {
        let (_, inputs) = test_inputs();
        let data = build_rpm(inputs).unwrap_or_else(|e| panic!("rpm 组包失败: {e}"));

        // rpm lead 魔数 ED AB EE DB
        assert!(
            data.len() > 4 && data[..4] == [0xED, 0xAB, 0xEE, 0xDB],
            "rpm 魔数不正确"
        );

        // rpm crate 解析回读
        let mut cursor = std::io::Cursor::new(data);
        let package =
            rpm::Package::parse(&mut cursor).unwrap_or_else(|e| panic!("rpm 解析回读失败: {e}"));
        let name = package
            .metadata
            .get_name()
            .unwrap_or_else(|e| panic!("读取 name 失败: {e}"));
        let version = package
            .metadata
            .get_version()
            .unwrap_or_else(|e| panic!("读取 version 失败: {e}"));
        let arch = package
            .metadata
            .get_arch()
            .unwrap_or_else(|e| panic!("读取 arch 失败: {e}"));
        assert_eq!(name, "foims-agent");
        assert_eq!(version, VERSION);
        assert_eq!(arch, "x86_64");
    }

    #[test]
    fn rpm_未知target报错() {
        let toml: &'static str = Box::leak("token = \"t\"\n".to_string().into_boxed_str());
        let ca: &'static [u8] = Box::leak(b"CA".to_vec().into_boxed_slice());
        let client_cert: &'static str = Box::leak("CLIENT CERT".to_string().into_boxed_str());
        let client_key: &'static str = Box::leak("CLIENT KEY".to_string().into_boxed_str());
        let binary: &'static [u8] = Box::leak(b"bin".to_vec().into_boxed_slice());
        let sh: &'static str = Box::leak("#!/bin/sh\n".to_string().into_boxed_str());
        let inputs = PackageInputs {
            binary,
            target: "riscv64-unknown-linux-musl",
            agent_version: VERSION,
            agent_toml: toml,
            ca_pem: ca,
            client_cert_pem: client_cert,
            client_key_pem: client_key,
            install_sh: sh,
        };
        assert!(
            build_deb(inputs.clone()).is_err(),
            "未知 target 的 deb 组包应失败"
        );
        assert!(build_rpm(inputs).is_err(), "未知 target 的 rpm 组包应失败");
    }

    #[test]
    fn temp_dir_唯一性_自检() {
        let a = temp_dir("selfcheck");
        let b = temp_dir("selfcheck");
        assert_ne!(a, b, "临时目录应唯一");
        std::fs::remove_dir_all(&a).unwrap_or_else(|e| panic!("清理临时目录失败: {e}"));
        std::fs::remove_dir_all(&b).unwrap_or_else(|e| panic!("清理临时目录失败: {e}"));
    }
}
