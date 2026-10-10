# F-OIMS — Organization Infrastructure Management System

F-OIMS (Organization Infrastructure Management System) is a high-performance organization infrastructure management platform built with Rust and modern web technologies. It provides network administrators with a secure, efficient, and intuitive one-stop management interface covering: IP addresses, switches, physical assets (server rooms / racks / workstations), organization & personnel, network topology visualization, and more.

[简体中文](README.md) | English

## ✨ Core Features

### 🚀 High-Performance Backend
- **Rust Workspace multi-crate architecture**: One main program plus 10 single-responsibility sub-crates, split by domain with clear boundaries.
- **Axum 0.8 + UDS/h2c**: The backend listens on a Unix Domain Socket and communicates over h2c (HTTP/2 cleartext); nginx reverse-proxy multiplexing eliminates handshake overhead.
- **PostgreSQL + SQLx**: Asynchronous database access with compile-time verified connection pool configuration and complete table-creation validation.

### 🛡️ Security First
- **Multi-layer authentication**: Local accounts + **LDAP** + **OIDC single sign-on (SSO)**, all supporting 2FA (TOTP) two-factor authentication.
- **Dual-token mechanism**: Short-lived Access Token + long-lived Refresh Token, with automatic silent renewal on the frontend.
- **Brute-force protection**: Application-layer fail2ban (multi-dimensional rate limiting and banning by IP/user/login/email), plus OS-level fail2ban configuration samples.
- **Password policies**: Password complexity and history-reuse checks; password expiry can be set per user, and expired accounts are automatically disabled.
- **Auditing & logging**: Operation logs, login logs, and notifications are recorded end-to-end, with bilingual output.

### 🌐 Resource & IP Management
- **IP lifecycle**: IPv4/IPv6 CIDR management.
- **Subnet management**: Flexible network Region and Subnet division.
- **Switch integration**: SNMP (v1/v2c/v3) automatic collection of switch information, port status, MAC tables, and LLDP neighbors.
- **Physical assets**: Complete management model for server rooms, racks, U-positions, and workstations, with full management of devices and cable links.

### 👥 Organization Management
- **Organization tree**: Multi-level organizational structure maintenance, employee roster, and organization templates.

### 📊 Visualization & Monitoring
- **Interactive layout**: SVG-based visual layout for server rooms / racks / workstations, with drag-and-drop adjustment and automatic drawing.
- **Dashboard**: Real-time statistics on network utilization, device distribution, and recent activity.
- **Host monitoring**: FOIMS Agent (multi-platform musl static binaries) reports CPU/memory/disk/network/temperature metrics over HTTP/3 mTLS; download installers, view the fleet list and detail curves in one place.

### 🛠️ System Management
- **Data portability**: Per-module CSV import/export and full database backup/restore.
- **Scheduled tasks**: Built-in cron scheduling (database backup, log cleanup, MAC sync, token cleanup, etc.).
- **Notification system**: In-app notifications and email notifications (SMTP) for timely alerts on IP/MAC changes.
- **Certificate management**: Self-signed certificate/CA generation, import, inventory, and download, working with nginx TLS deployment.
- **Service management**: One-click restart of systemd services from the web UI.
- **Internationalization**: Multi-language support across frontend and backend; Chinese and English are currently implemented.

## 🧭 Architecture

- The backend listens on a UDS over h2c (default `/run/foims/api.sock`) and does not directly expose a TCP port.
- Two exceptions are in-process UDP listeners (not proxied by nginx; allow them through the firewall): Agent metric reporting (UDP 9100 by default, HTTP/3) and SNMP Trap reception (UDP 162 by default, configurable).
- TLS certificates, security response headers, and static asset caching are all handled at the nginx layer; see samples in [deploy/nginx/](deploy/nginx/).

## 📦 Installation & Deployment

### Prerequisites
- **Rust**: 1.87 or higher (recommended to install via the official rustup command)
- **PostgreSQL**: version 16 or higher
- **OpenSSL**: development libraries (libssl-dev, required by the native-tls mail component)
- **nginx**: version ≥ 1.28.1 (h2c upstream proxy; QUIC support must be compiled in for HTTP/3)
- **OS**: Linux (Debian 11+ or Ubuntu 24.04+ recommended)

### 1. Get the Code and Configure

```bash
git clone https://github.com/o-i-i-o/foims.git
cd foims
cp config.toml.example config.toml
```

The configuration file is searched by priority: `/etc/foims/config.toml` → `/opt/foims/config.toml` → `./config.toml`. See [🔧 Configuration](#-configuration) for details.

### 2. Prepare the Database

The database must be created manually in advance: run the provided `scripts/init-pgsql.sh` script
(usage: `PG_PASSWORD=your_password ./scripts/init-pgsql.sh`, optionally adjusted via the
`PG_USER`, `PG_DATABASE`, `PG_HOST`, and `PG_PORT` environment variables), or execute the
database-creation SQL manually in PostgreSQL.

Keep `[init] enabled = true` in `config.toml`, then visit the initialization wizard after the first start:
after completing the PostgreSQL checks, fill in the connection information on the "Database Configuration" page and proceed via "Connection Test"
(which verifies connectivity and requires the account to be the database owner with the CREATEDB privilege).

### 3. Build and Run (Development / Debugging)

```bash
cargo build --release
sudo ./target/release/foims          # UDS binding and group ownership require root
```

- The backend only serves the API (over UDS); static assets and TLS are hosted by nginx:
  install and configure per [deploy/nginx/foims.conf](deploy/nginx/foims.conf) (adjust
  `web_dir` and `uds_path` as needed), then run `nginx -t && systemctl reload nginx` and visit the site.

### 4. Initialize the System
- Initialize the database first; this project provides the `scripts/init-pgsql.sh` script to help create the database
- On first visit, make sure `init.enabled` is `true` in `config.toml`; you will enter the initialization wizard page — follow the prompts to create the tables and the administrator account
- After initialization, log in with that account; it is recommended to change the password immediately and enable 2FA in personal settings

**To avoid security risks, the sample nginx configuration restricts the initialization API endpoints to 127.0.0.1 (localhost) only. Please manually add the actual IP of your administration terminal, and remove it after initialization is complete.**

### 5. Production Deployment

1. **Build the release binary**
   ```bash
   cargo build --release
   ```

2. **Install the production nginx configuration**
   - See [deploy/nginx/foims.conf](deploy/nginx/foims.conf): configure `server_name`, the static asset directory, and TLS certificate paths
   - TLS certificates can be requested automatically with certbot, or generated as self-signed certificates in "System Settings → Certificate Management" and downloaded for deployment
   - The backend UDS path must match the nginx upstream (default `/run/foims/api.sock`)

3. **Register the systemd service**
   - Install the deb package, or manually place `foims.service` at `/etc/systemd/system/foims.service` to register the service


4. **Enable and start**
   ```bash
   systemctl enable --now foims
   ```

5. **(Optional) Enable OS-level fail2ban**
   - See samples in [deploy/fail2ban/](deploy/fail2ban/) (filter and jail configurations)

## 🔧 Configuration

The configuration file is in TOML format (search paths as above); for all fields and comments see [config.toml.example](config.toml.example).

## 🚀 Quick Start

1. **Initialize the system**: On first visit, enter the initialization wizard to create the tables and the administrator account
2. **Log in**: Sign in with the initialized administrator account
3. **Maintain the organization**: Maintain the organization tree and employee roster
4. **Configure subnets**: Add network regions and subnets under "Resource Management" to divide IP ranges
5. **Manage devices**: Add switches under "Switch Management" and configure SNMP to automatically collect ports, MAC tables, and LLDP
6. **Manage physical assets**: Enter server rooms, racks, and workstations, and manage devices and cable links
7. **Monitor & operate**: View the dashboard, configure notifications, and schedule backup tasks

## 📚 Documentation

- **Code style guide**: [docs/code-style.md](docs/code-style.md) (the single authoritative source for both frontend and backend)
- **Collaboration & build notes**: [AGENTS.md](AGENTS.md)
- **Deployment samples**: [deploy/](deploy/) (nginx, fail2ban)


## 📝 License

This project is licensed under the [GPL-3.0-or-later license](LICENSE).

Copyright (c) 2025-2026 oi-io <boss@oi-io.cc>

This program is free software; you can redistribute it and/or modify it under the terms of the GNU GPL v3 (or any later version) published by the Free Software Foundation. See the [LICENSE](LICENSE) file for details.

## 📦 Third-Party Components

This project uses the following open-source components. For details, see the [NOTICE](NOTICE) file; full license texts are in the [third-party-licenses/](third-party-licenses/) directory:

- **Axum** - Rust web framework (MIT)
- **SQLx** - Async PostgreSQL driver (Apache-2.0 OR MIT)
- **Tokio** - Async runtime (MIT)
- **ldap3 / openidconnect** - LDAP and OIDC single sign-on (MIT OR Apache-2.0 / MIT)
- **async-snmp** - SNMP client (Apache-2.0 OR MIT)
- **rcgen / x509-parser** - X.509 certificate generation and parsing (MIT OR Apache-2.0)

## 👤 Author

**oi-io** - [boss@oi-io.cc](mailto:boss@oi-io.cc)

## 🤝 Contributing

Issues and Pull Requests are welcome to help improve this project! Before submitting, please read [docs/code-style.md](docs/code-style.md) and make sure `cargo fmt`, `cargo clippy --release -- -D warnings`, and the frontend lint all pass.

## 📞 Support

If you run into problems, please reach out through the following channels:
- Submit an Issue on the GitHub repository
- QQ group: 1107983881
- Email boss@oi-io.cc
- If this program helps you, please consider donating to support development
![Donation QR Code](/mm_reward_qrcode_1790828899753.png)
- Paid deployment services are available — email me for details
---

**FOIMS — Making organization infrastructure management easier!**
