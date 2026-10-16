FOIMS Agent (Windows)
=====================

安装步骤：

1. 从 FOIMS 服务端「主机监控」页下载「配置包（zip）」并解压，
   得到四个配置文件：
     agent.toml   上报配置（server_addr/token/采集与上报间隔）
     ca.pem       站点 CA 证书（mTLS 信任锚）
     client.pem   客户端证书
     client.key   客户端私钥（敏感物料，请勿泄露）

2. 将 foims-agent.exe 与上述四个文件放到同一目录，
   例如 C:\foims-agent\。

3. 启动（二选一）：
   - 手动运行：
       foims-agent.exe --config C:\foims-agent\agent.toml --cert-dir C:\foims-agent
   - 注册为 Windows 服务（需管理员 PowerShell，使用 sc.exe）：
       sc.exe create foims-agent binPath= "C:\foims-agent\foims-agent.exe --config C:\foims-agent\agent.toml --cert-dir C:\foims-agent" start= auto
       sc.exe start foims-agent

注意：agent.toml 中的 token 与 client.key 属敏感物料，请妥善保管
（目录权限建议仅管理员可读）。
