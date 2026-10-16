//! Agent 配置包组包（zip / deb / rpm）。
//!
//! 二进制安装包由 agent 工作空间 CI 直出（deb/rpm/zip，按需从 Release 页
//! 下载）；本模块只按 Web 页所选操作系统现场组「配置包」，内容统一为
//! agent.toml + ca.pem + client.pem + client.key：
//! - zip：linux + Windows 通用（文件平铺，解压到 foims-agent.exe 同目录，
//!   Linux 随附 uninstall.sh 卸载脚本）；
//! - deb：Architecture: all，强制依赖 foims-agent，文件落 /etc/foims-agent/
//!   （权限 0600/0644）；
//! - rpm：noarch，Requires foims-agent，内容与 deb 等价，agent.toml 标记
//!   config。
//!
//! 安装闭环：依赖强制程序包先装，配置包 postinst/%post 统一 enable +
//! restart foims-agent 服务使配置即刻生效（首次安装直接拉起）。
//!
//! 全部在内存中完成，不落临时文件。

use std::io::Write as _;

use flate2::write::GzEncoder;

/// 组包统一输入（配置与证书物料）。
#[derive(Clone)]
pub struct PackageInputs<'a> {
    /// 完整 agent.toml 文本（含 server_addr/token/间隔）
    pub agent_toml: &'a str,
    /// 站点 CA 公钥
    pub ca_pem: &'a [u8],
    /// Agent 客户端证书 PEM（mTLS 上报身份，全站共用一张）
    pub client_cert_pem: &'a str,
    /// Agent 客户端私钥 PEM（0600 语义落位）
    pub client_key_pem: &'a str,
}

/// zip 包内 README.txt（linux + Windows 通用安装/卸载说明）。
pub const ZIP_README: &str = r#"FOIMS Agent 配置包
==================

本压缩包仅含配置与 mTLS 证书，需配合 foims-agent 程序本体使用
（程序本体请从发布页按平台下载安装）：

  agent.toml    上报配置（server_addr/token/采集与上报间隔）
  ca.pem        站点 CA 证书（mTLS 信任锚）
  client.pem    客户端证书
  client.key    客户端私钥（敏感物料，请勿泄露）
  uninstall.sh  Linux 卸载脚本（Windows 用不到，可忽略）

Linux 安装：以 root 将四个配置文件复制到 /etc/foims-agent/ 后重启服务：
  systemctl restart foims-agent
（服务由 foims-agent 程序包提供；配置更新后同样重启生效。）

Windows 安装：将四个配置文件解压到 foims-agent.exe 同一目录，经程序包
内 install.cmd 注册 Windows 服务；配置更新后重启服务生效：
  sc.exe stop foims-agent
  sc.exe start foims-agent

卸载：
  Linux：以 root 运行本包内 uninstall.sh（停止并禁用服务、删除
  /etc/foims-agent；程序本体请用系统包管理器卸载）。
  Windows：以管理员运行程序包内 uninstall.cmd。

注意：client.key 与 agent.toml 中的 token 属敏感物料，请妥善保管。
"#;

/// systemd 服务重启脚本片段（deb postinst / rpm post scriptlet 共用）：
/// 配置包强制依赖 foims-agent（deb Depends / rpm Requires），安装时程序包
/// 与 unit 文件必然已就位；此处确保服务自启并重启一次，使新配置即刻生效
/// （首次安装由本脚本直接拉起，升级安装则无缝换用新配置，无需手动操作）。
pub const RESTART_SNIPPET: &str = r#"#!/bin/sh
set -e
if command -v systemctl >/dev/null 2>&1 && systemctl list-unit-files foims-agent.service >/dev/null 2>&1; then
    systemctl daemon-reload || true
    systemctl enable foims-agent.service >/dev/null 2>&1 || true
    systemctl restart foims-agent.service || true
fi
"#;

/// zip 配置包随附的 Linux 卸载脚本（uninstall.sh，仅手动解压安装路径用）：
/// 配置经 deb/rpm 安装（foims-agent-config）时交回包管理器，拒绝双轨
/// 卸载；否则停止并禁用服务、删除 /etc/foims-agent/。程序本体（unit、
/// /usr/bin/foims-agent）属程序包，始终提示用系统包管理器卸载。
pub const UNINSTALL_SH: &str = r#"#!/bin/sh
# FOIMS Agent 配置卸载脚本（Linux 通用 zip 配置包随附；需 root 执行）
set -e

if [ "$(id -u)" -ne 0 ]; then
    echo "错误：请以 root 身份运行本脚本。" >&2
    exit 1
fi

# 配置经系统包管理器安装时交回包管理器，避免双轨卸载残留
if dpkg -s foims-agent-config >/dev/null 2>&1 || rpm -q foims-agent-config >/dev/null 2>&1; then
    echo "错误：配置包已由系统包管理器安装（foims-agent-config），请用 apt/rpm 卸载，不要运行本脚本。" >&2
    exit 1
fi

if command -v systemctl >/dev/null 2>&1; then
    systemctl stop foims-agent.service 2>/dev/null || true
    systemctl disable foims-agent.service 2>/dev/null || true
    systemctl daemon-reload 2>/dev/null || true
fi

rm -rf /etc/foims-agent
echo "卸载完成：服务已停止并禁用，配置目录 /etc/foims-agent 已删除。"
echo "程序本体（foims-agent）请用系统包管理器卸载：apt remove foims-agent 或 rpm -e foims-agent。"
"#;

/// deb 包控制文件 control 文本（配置包，Architecture: all）。
fn deb_control(agent_version: &str) -> String {
    format!(
        "Package: foims-agent-config\n\
         Version: {agent_version}\n\
         Section: net\n\
         Priority: optional\n\
         Architecture: all\n\
         Depends: foims-agent\n\
         Maintainer: oi-io <boss@oi-io.cc>\n\
         Description: FOIMS Agent configuration package\n\
         \x20Server-issued configuration and mTLS material for foims-agent.\n"
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

/// data.tar.gz 公共内容：/etc/foims-agent/ 下 agent.toml(0600) / ca.pem(0644) /
/// client.pem(0644) / client.key(0600)。
fn data_tar(inputs: &PackageInputs<'_>) -> Result<Vec<u8>, String> {
    gzip_tar(|builder| {
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
        Ok(())
    })
}

/// 组装 zip 配置包（linux + Windows 通用）：四个配置文件平铺 + README.txt +
/// uninstall.sh（Linux 卸载脚本，0755），文件名不带目录前缀。
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

    start(&mut writer, "README.txt", 0o644)?;
    writer
        .write_all(ZIP_README.as_bytes())
        .map_err(|e| format!("zip 写入 README.txt 内容失败: {e}"))?;

    start(&mut writer, "uninstall.sh", 0o755)?;
    writer
        .write_all(UNINSTALL_SH.as_bytes())
        .map_err(|e| format!("zip 写入 uninstall.sh 内容失败: {e}"))?;

    writer
        .finish()
        .map_err(|e| format!("zip 收尾失败: {e}"))
        .map(std::io::Cursor::into_inner)
}

/// 组装 deb 配置包（纯 Rust，不调用外部命令；Architecture: all）。
///
/// data.tar.gz：/etc/foims-agent/ 四文件；control.tar.gz：control(0644) +
/// postinst(0755，重启已装 agent)；ar 成员依次：debian-binary、
/// control.tar.gz、data.tar.gz。
pub fn build_deb(agent_version: &str, inputs: &PackageInputs<'_>) -> Result<Vec<u8>, String> {
    let data_gz = data_tar(inputs)?;

    let control_gz = gzip_tar(|builder| {
        append_tar_entry(
            builder,
            "./control",
            deb_control(agent_version).as_bytes(),
            0o644,
        )?;
        append_tar_entry(builder, "./postinst", RESTART_SNIPPET.as_bytes(), 0o755)?;
        Ok(())
    })?;

    let mut out = Vec::new();
    out.extend_from_slice(b"!<arch>\n");
    append_ar_member(&mut out, "debian-binary", b"2.0\n")?;
    append_ar_member(&mut out, "control.tar.gz", &control_gz)?;
    append_ar_member(&mut out, "data.tar.gz", &data_gz)?;
    Ok(out)
}

/// 组装 rpm 配置包（rpm crate，不签名；noarch）。
///
/// 安装文件与 deb 等价；agent.toml 标记 config；post scriptlet 与 deb
/// postinst 同语义。
pub fn build_rpm(agent_version: &str, inputs: &PackageInputs<'_>) -> Result<Vec<u8>, String> {
    let mut builder = rpm::PackageBuilder::new(
        "foims-agent-config",
        agent_version,
        "GPL-3.0-or-later",
        "noarch",
        "FOIMS Agent configuration package",
    );
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
    builder.requires(rpm::Dependency::any("foims-agent"));
    builder.post_install_script(RESTART_SNIPPET);

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

    const VERSION: &str = "0.25.0";

    /// 组包测试输入
    fn test_inputs() -> PackageInputs<'static> {
        // 泄漏测试常量避免自引用生命周期问题（进程退出即回收）
        let toml: &'static str = Box::leak(
            "server_addr = \"10.0.0.1:9100\"\ntoken = \"tok\"\n"
                .to_string()
                .into_boxed_str(),
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
        PackageInputs {
            agent_toml: toml,
            ca_pem: ca,
            client_cert_pem: client_cert,
            client_key_pem: client_key,
        }
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

    /// 解压 tar.gz 并返回 (路径, mode, 内容) 列表
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
    fn zip_读回验证条目与权限() {
        let inputs = test_inputs();
        let data = build_zip(inputs).unwrap_or_else(|e| panic!("zip 组包失败: {e}"));

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data))
            .unwrap_or_else(|e| panic!("zip 读回失败: {e}"));
        let expected = [
            ("agent.toml", 0o600),
            ("ca.pem", 0o644),
            ("client.pem", 0o644),
            ("client.key", 0o600),
            ("README.txt", 0o644),
            ("uninstall.sh", 0o755),
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
        let inputs = test_inputs();
        let data = build_deb(VERSION, &inputs).unwrap_or_else(|e| panic!("deb 组包失败: {e}"));

        let members = parse_ar(&data).unwrap_or_else(|e| panic!("ar 解析失败: {e}"));
        let names: Vec<&str> = members.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["debian-binary", "control.tar.gz", "data.tar.gz"]);
        assert_eq!(members[0].1, b"2.0\n", "debian-binary 内容应为 2.0");

        // control.tar.gz：control 与 postinst（tar crate 写入时归一掉 "./" 前缀）
        let control_entries =
            parse_tar_gz(&members[1].1).unwrap_or_else(|e| panic!("control.tar.gz 解析失败: {e}"));
        assert_eq!(control_entries.len(), 2, "control.tar.gz 应有 2 个条目");
        assert_eq!(control_entries[0].0, "control");
        assert_eq!(control_entries[0].1, 0o644);
        let control_text = String::from_utf8_lossy(&control_entries[0].2);
        assert!(control_text.contains("Package: foims-agent-config"));
        assert!(control_text.contains(&format!("Version: {VERSION}")));
        assert!(control_text.contains("Architecture: all"));
        assert!(
            control_text.contains("Depends: foims-agent"),
            "配置包应强制依赖 foims-agent 程序包"
        );
        assert_eq!(control_entries[1].0, "postinst");
        assert_eq!(control_entries[1].1, 0o755);
        let postinst = String::from_utf8_lossy(&control_entries[1].2);
        assert!(
            postinst.contains("systemctl enable foims-agent.service")
                && postinst.contains("systemctl restart foims-agent.service"),
            "postinst 应启用并重启 foims-agent 使配置即刻生效（首次安装直接拉起）"
        );
        assert!(
            !postinst.contains("try-restart"),
            "try-restart 对未运行的服务是 no-op，不得用于首次拉起"
        );

        // data.tar.gz：/etc/foims-agent/ 四文件与权限
        let data_entries =
            parse_tar_gz(&members[2].1).unwrap_or_else(|e| panic!("data.tar.gz 解析失败: {e}"));
        let expect_files = [
            ("etc/foims-agent/agent.toml", 0o600),
            ("etc/foims-agent/ca.pem", 0o644),
            ("etc/foims-agent/client.pem", 0o644),
            ("etc/foims-agent/client.key", 0o600),
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
        assert_eq!(
            data_entries[0].2,
            inputs.agent_toml.as_bytes(),
            "agent.toml 内容应一致"
        );
    }

    #[test]
    fn rpm_魔数与元数据读回验证() {
        let inputs = test_inputs();
        let data = build_rpm(VERSION, &inputs).unwrap_or_else(|e| panic!("rpm 组包失败: {e}"));

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
        assert_eq!(name, "foims-agent-config");
        assert_eq!(version, VERSION);
        assert_eq!(arch, "noarch");

        // Requires 应包含对 foims-agent 的强制依赖（任意版本）
        let requires = package
            .metadata
            .get_requires()
            .unwrap_or_else(|e| panic!("读取 requires 失败: {e}"));
        assert!(
            requires.iter().any(|d| d.name == "foims-agent"),
            "rpm 配置包应强制依赖 foims-agent 程序包，实际: {requires:?}"
        );
    }
}
