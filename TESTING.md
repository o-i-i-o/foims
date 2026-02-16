# 测试说明

本次更新包含了后端登录逻辑的重构、前端页面的改进、系统重启功能的修复、**自签名证书生成的优化**、**前端下载功能 Token 自动刷新支持**以及**证书管理界面的 UI 优化**。以下是验证修改结果的步骤。

## 1. 后端测试

运行单元测试确保代码编译通过且逻辑正确：

```bash
cargo test
```

## 2. API 功能验证

启动服务器：

```bash
cargo run
```

### 2.1 验证标准登录 (密码)

使用 curl 或 Postman 测试标准密码登录：

```bash
curl -k -X POST https://localhost/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "admin@123", "remember_me": true}'
```

预期结果：返回 `success: true` 和 `access_token`。

### 2.2 验证邮箱验证码登录

1. **发送验证码**

```bash
curl -k -X POST https://localhost/api/auth/login/send-code \
  -H "Content-Type: application/json" \
  -d '{"email": "boss@oi-io.cc"}'
```

预期结果：`{"success":true,"message":"验证码已发送","data":null}`

2. **使用验证码登录**

**测试错误验证码：**

```bash
curl -k -X POST https://localhost/api/auth/login/email \
  -H "Content-Type: application/json" \
  -d '{"email": "boss@oi-io.cc", "code": "000000"}'
```

预期结果：`{"success":false,"message":"验证码无效或已过期"}` (或类似错误信息)

### 2.3 验证当前用户信息 (/me)

使用登录获取的 Token：

```bash
curl -k -X GET https://localhost/api/auth/me \
  -H "Authorization: Bearer <YOUR_ACCESS_TOKEN>"
```

预期结果：返回当前用户信息。

## 3. 系统功能验证

### 3.1 验证服务重启 (Standalone 模式)

1. 登录系统后，进入“系统管理” -> “系统配置”。
2. 修改任意配置，点击“保存配置”。
3. 点击“重启应用系统”。
4. **预期结果**：
   - 界面弹出“服务重启命令已发送，服务正在重启”。
   - 页面自动刷新后，**不应再出现**“配置已更新”的黄色提示。

### 3.2 验证仪表盘跳转

1. 进入仪表盘页面。
2. 点击“Top Networks”或“Total Networks”卡片。
3. **预期结果**：成功跳转到资源管理页面，并选中“Networks”标签。
4. 跳转后点击侧边栏其他链接，能正常跳转。

### 3.3 验证自签名证书优化 (TLS 1.3 / HTTP/3 适配)

1. 登录系统，进入“系统管理” -> “系统配置”。
2. 确保 **Public URL** 已设置（例如 `https://192.168.1.100` 或 `https://myserver.com`）。
3. 点击“更新证书” -> 选择“生成自签名证书”。
4. 输入必要的 Common Name (CN)，点击生成。
5. 生成成功后，点击“重启应用系统”。
6. 系统重启后，点击新增的 **“下载 CA”** 按钮。
7. 下载 `ca.pem` 文件，并使用 OpenSSL 查看其内容：

```bash
openssl x509 -in ca.pem -text -noout
```

**验证点：**
*   **Version**: 必须为 `3 (0x2)`。
*   **Signature Algorithm**: 必须为 `ecdsa-with-SHA256`。
*   **Subject Public Key Info**: 必须为 `id-ecPublicKey` (P-256)。
*   **Validity**: `Not Before` 为当前时间，`Not After` 为指定时间（默认 10 年）。
*   **X509v3 Extensions**:
    *   **Subject Alternative Name (SAN)**: 必须包含配置的 IP 或域名（如 `IP Address:192.168.1.100`）。
    *   **Extended Key Usage**: 必须包含 `TLS Web Server Authentication`。
    *   **Key Usage**: 必须包含 `Digital Signature`。

### 3.4 验证前端下载功能 Token 刷新

1. 登录系统。
2. 等待 Access Token 过期（或手动清除 Token 模拟过期）。
3. 点击“下载 CA”按钮。
4. **预期结果**：
   - 前端应自动尝试刷新 Token。
   - 如果刷新成功，下载应自动继续并成功。
   - 如果刷新失败，应跳转到登录页或显示“登录已过期”提示。
   - 不应再直接报错 `api.token_expired` 而无任何处理。

### 3.5 验证证书界面交互

1. 进入“系统配置”。
2. 将 **证书类型** 切换为 **自签名证书**。
   - **预期结果**：显示“更新证书”和“下载 CA”按钮，隐藏“导入证书”按钮。
3. 将 **证书类型** 切换为 **导入证书**。
   - **预期结果**：隐藏“更新证书”和“下载 CA”按钮，显示“导入证书”（或“更新”）按钮。

## 修改摘要

- **src/system/config.rs**:
  - 重写了 `generate_self_signed_cert`：
    - 强制使用 X.509 v3。
    - 算法升级为 ECDSA P-256 (TLS 1.3 推荐)。
    - 自动从 `public_url` 提取 IP 或域名写入 SAN 扩展。
    - 添加了 EKU (ServerAuth) 和 KeyUsage (DigitalSignature)。
    - 修复了证书有效期问题（默认 10 年，从当前时间开始）。
  - 新增 `download_certificate` API，支持下载当前的 CA 证书。
- **web/static/js/utils/apiClient.js**:
  - 增强 `apiRequest`，支持 `application/x-pem-file` 和其他文件类型的自动处理。
  - 完善了文件名的提取逻辑。
- **web/static/js/modules/systemManager.js**:
  - 将所有文件下载函数（证书、CSV、数据库、配置、模板）从原生 `fetch` 迁移到 `apiRequest`。
  - 实现了 Token 自动刷新和统一的错误处理。
  - **UI 优化**：在 `handleCertTypeChange` 中根据证书类型动态显示/隐藏“下载 CA”按钮。
- **src/routes/mod.rs**: 注册了 `/certificate/download` 路由。
- **web/static/main.html**: 系统配置页面新增“下载 CA”按钮。
