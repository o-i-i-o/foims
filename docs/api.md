# IPMA API 文档

## 概述

本文档描述了 IPMA (IP Management Application) 系统的 API 接口规范。IPMA 是一个 IP 地址管理系统，提供了完整的资源管理、认证、系统管理等功能。

## 基础信息

### 1. 基本 URL

所有 API 请求的基础 URL 为：
- 开发环境：`http://localhost/api`
- 生产环境：`https://your-domain/api`

### 2. 认证方式

系统使用 **HttpOnly Cookie** 进行认证，API 调用时需设置 `credentials: 'include'`。

### 3. 响应格式

所有 API 响应遵循统一格式：

#### 成功响应

```json
{
  "success": true,
  "data": {},
  "message": "操作成功"
}
```

#### 错误响应

```json
{
  "success": false,
  "message": "错误信息",
  "error_type": "error_code"
}
```

### 4. 错误码

| 错误码 | 描述 | HTTP 状态码 |
|--------|------|------------|
| `unauthorized` | 认证失败 | 401 |
| `forbidden` | 权限不足 | 403 |
| `not_found` | 资源不存在 | 404 |
| `validation_error` | 参数验证失败 | 400 |
| `database_error` | 数据库错误 | 500 |
| `internal_error` | 服务器内部错误 | 500 |

## API 接口

### 1. 认证相关 API

#### 1.1 登录

**请求**：
- URL: `/api/auth/login`
- 方法: `POST`
- 内容类型: `application/json`

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `username` | string | 是 | 用户名或邮箱 |
| `password` | string | 是 | 密码 |
| `remember_me` | boolean | 否 | 是否保持登录状态 |

**响应**：

```json
{
  "success": true,
  "data": {
    "user": {
      "id": 1,
      "username": "admin",
      "email": "admin@example.com",
      "role": "admin",
      "created_at": "2024-01-01T00:00:00"
    },
    "requires_two_factor": false
  },
  "message": "登录成功"
}
```

#### 1.2 邮箱登录

**请求**：
- URL: `/api/auth/login/email`
- 方法: `POST`
- 内容类型: `application/json`

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `email` | string | 是 | 邮箱地址 |
| `code` | string | 是 | 验证码 |
| `remember_me` | boolean | 否 | 是否保持登录状态 |

**响应**：

```json
{
  "success": true,
  "data": {
    "user": {
      "id": 1,
      "username": "admin",
      "email": "admin@example.com",
      "role": "admin",
      "created_at": "2024-01-01T00:00:00"
    },
    "username": "admin",
    "requires_two_factor": false
  },
  "message": "登录成功"
}
```

#### 1.3 发送登录验证码

**请求**：
- URL: `/api/auth/login/send-code`
- 方法: `POST`
- 内容类型: `application/json`

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `email` | string | 是 | 邮箱地址 |

**响应**：

```json
{
  "success": true,
  "message": "验证码已发送"
}
```

#### 1.4 双因素认证登录

**请求**：
- URL: `/api/auth/login/two-factor`
- 方法: `POST`
- 内容类型: `application/json`

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `username` | string | 是 | 用户名 |
| `password` | string | 否 | 密码（邮箱登录时可为空） |
| `two_factor_code` | string | 是 | 双因素验证码 |
| `remember_me` | boolean | 否 | 是否保持登录状态 |

**响应**：

```json
{
  "success": true,
  "data": {
    "user": {
      "id": 1,
      "username": "admin",
      "email": "admin@example.com",
      "role": "admin",
      "created_at": "2024-01-01T00:00:00"
    }
  },
  "message": "登录成功"
}
```

#### 1.5 发送双因素验证码

**请求**：
- URL: `/api/auth/login/send-2fa-code`
- 方法: `POST`
- 内容类型: `application/json`

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `username` | string | 是 | 用户名 |

**响应**：

```json
{
  "success": true,
  "message": "验证码已发送"
}
```

#### 1.6 登出

**请求**：
- URL: `/api/auth/logout`
- 方法: `POST`

**响应**：

```json
{
  "success": true,
  "message": "登出成功"
}
```

#### 1.7 刷新令牌

**请求**：
- URL: `/api/auth/refresh`
- 方法: `POST`

**响应**：

```json
{
  "success": true,
  "message": "令牌刷新成功"
}
```

#### 1.8 忘记密码

**请求**：
- URL: `/api/auth/forgot-password`
- 方法: `POST`
- 内容类型: `application/json`

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `email` | string | 是 | 邮箱地址 |

**响应**：

```json
{
  "success": true,
  "message": "密码重置邮件已发送"
}
```

#### 1.9 重置密码

**请求**：
- URL: `/api/auth/reset-password`
- 方法: `POST`
- 内容类型: `application/json`

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `token` | string | 是 | 重置令牌 |
| `password` | string | 是 | 新密码 |

**响应**：

```json
{
  "success": true,
  "message": "密码重置成功"
}
```

#### 1.10 获取当前用户信息

**请求**：
- URL: `/api/auth/me`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "username": "admin",
    "email": "admin@example.com",
    "role": "admin",
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "获取成功"
}
```

### 2. 用户管理 API

#### 2.1 获取用户列表

**请求**：
- URL: `/api/users`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "username": "admin",
      "email": "admin@example.com",
      "role": "admin",
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

#### 2.2 创建用户

**请求**：
- URL: `/api/users`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `username` | string | 是 | 用户名 |
| `email` | string | 是 | 邮箱地址 |
| `password` | string | 是 | 密码 |
| `role` | string | 是 | 角色 |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 2,
    "username": "user1",
    "email": "user1@example.com",
    "role": "user",
    "created_at": "2024-01-02T00:00:00"
  },
  "message": "创建成功"
}
```

#### 2.3 获取用户详情

**请求**：
- URL: `/api/users/{id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "username": "admin",
    "email": "admin@example.com",
    "role": "admin",
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "获取成功"
}
```

#### 2.4 更新用户

**请求**：
- URL: `/api/users/{id}`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `username` | string | 否 | 用户名 |
| `email` | string | 否 | 邮箱地址 |
| `password` | string | 否 | 密码 |
| `role` | string | 否 | 角色 |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "username": "admin",
    "email": "admin@example.com",
    "role": "admin",
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "更新成功"
}
```

#### 2.5 删除用户

**请求**：
- URL: `/api/users/{id}`
- 方法: `DELETE`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "删除成功"
}
```

### 3. 双因素认证管理 API

#### 3.1 初始化双因素认证

**请求**：
- URL: `/api/two-factor/init`
- 方法: `POST`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "secret": "JBSWY3DPEHPK3PXP",
    "qr_code": "data:image/png;base64,..."
  },
  "message": "初始化成功"
}
```

#### 3.2 启用双因素认证

**请求**：
- URL: `/api/two-factor/enable`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `code` | string | 是 | 双因素验证码 |
| `secret` | string | 是 | 密钥 |

**响应**：

```json
{
  "success": true,
  "message": "双因素认证已启用"
}
```

#### 3.3 禁用双因素认证

**请求**：
- URL: `/api/two-factor/disable`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `code` | string | 是 | 双因素验证码 |

**响应**：

```json
{
  "success": true,
  "message": "双因素认证已禁用"
}
```

### 4. 资源管理 API

#### 4.1 网络管理

##### 4.1.1 获取网络列表

**请求**：
- URL: `/api/resources/networks`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "name": "192.168.1.0/24",
      "description": "办公网络",
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

##### 4.1.2 创建网络

**请求**：
- URL: `/api/resources/networks`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 是 | 网络名称（CIDR格式） |
| `description` | string | 否 | 描述 |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 2,
    "name": "192.168.2.0/24",
    "description": "测试网络",
    "created_at": "2024-01-02T00:00:00"
  },
  "message": "创建成功"
}
```

##### 4.1.3 获取网络详情

**请求**：
- URL: `/api/resources/networks/{id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "192.168.1.0/24",
    "description": "办公网络",
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "获取成功"
}
```

##### 4.1.4 更新网络

**请求**：
- URL: `/api/resources/networks/{id}`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 否 | 网络名称 |
| `description` | string | 否 | 描述 |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "192.168.1.0/24",
    "description": "更新后的办公网络",
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "更新成功"
}
```

##### 4.1.5 删除网络

**请求**：
- URL: `/api/resources/networks/{id}`
- 方法: `DELETE`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "删除成功"
}
```

#### 4.2 网络区域管理

##### 4.2.1 获取网络区域列表

**请求**：
- URL: `/api/resources/network-regions`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "name": "A区",
      "description": "A区域网络",
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

##### 4.2.2 创建网络区域

**请求**：
- URL: `/api/resources/network-regions`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 是 | 区域名称 |
| `description` | string | 否 | 描述 |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 2,
    "name": "B区",
    "description": "B区域网络",
    "created_at": "2024-01-02T00:00:00"
  },
  "message": "创建成功"
}
```

##### 4.2.3 获取网络区域详情

**请求**：
- URL: `/api/resources/network-regions/{id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "A区",
    "description": "A区域网络",
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "获取成功"
}
```

##### 4.2.4 更新网络区域

**请求**：
- URL: `/api/resources/network-regions/{id}`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 否 | 区域名称 |
| `description` | string | 否 | 描述 |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "A区",
    "description": "更新后的A区域网络",
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "更新成功"
}
```

##### 4.2.5 删除网络区域

**请求**：
- URL: `/api/resources/network-regions/{id}`
- 方法: `DELETE`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "删除成功"
}
```

##### 4.2.6 获取网络区域下的机柜

**请求**：
- URL: `/api/resources/network-regions/{id}/cabinets`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "name": "机柜1",
      "network_region_id": 1,
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

#### 4.3 房间管理

##### 4.3.1 获取房间列表

**请求**：
- URL: `/api/resources/rooms`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "name": "办公室1",
      "description": "第一办公室",
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

##### 4.3.2 创建房间

**请求**：
- URL: `/api/resources/rooms`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 是 | 房间名称 |
| `description` | string | 否 | 描述 |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 2,
    "name": "办公室2",
    "description": "第二办公室",
    "created_at": "2024-01-02T00:00:00"
  },
  "message": "创建成功"
}
```

##### 4.3.3 获取房间详情

**请求**：
- URL: `/api/resources/rooms/{id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "办公室1",
    "description": "第一办公室",
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "获取成功"
}
```

##### 4.3.4 更新房间

**请求**：
- URL: `/api/resources/rooms/{id}`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 否 | 房间名称 |
| `description` | string | 否 | 描述 |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "办公室1",
    "description": "更新后的第一办公室",
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "更新成功"
}
```

##### 4.3.5 删除房间

**请求**：
- URL: `/api/resources/rooms/{id}`
- 方法: `DELETE`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "删除成功"
}
```

##### 4.3.6 获取房间下的网络

**请求**：
- URL: `/api/resources/rooms/{id}/networks`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "name": "192.168.1.0/24",
      "description": "办公网络",
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

#### 4.4 机柜管理

##### 4.4.1 获取机柜列表

**请求**：
- URL: `/api/resources/cabinets`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "name": "机柜1",
      "network_region_id": 1,
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

##### 4.4.2 创建机柜

**请求**：
- URL: `/api/resources/cabinets`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 是 | 机柜名称 |
| `network_region_id` | integer | 是 | 网络区域ID |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 2,
    "name": "机柜2",
    "network_region_id": 1,
    "created_at": "2024-01-02T00:00:00"
  },
  "message": "创建成功"
}
```

##### 4.4.3 获取机柜详情

**请求**：
- URL: `/api/resources/cabinets/{id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "机柜1",
    "network_region_id": 1,
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "获取成功"
}
```

##### 4.4.4 更新机柜

**请求**：
- URL: `/api/resources/cabinets/{id}`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 否 | 机柜名称 |
| `network_region_id` | integer | 否 | 网络区域ID |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "机柜1",
    "network_region_id": 1,
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "更新成功"
}
```

##### 4.4.5 删除机柜

**请求**：
- URL: `/api/resources/cabinets/{id}`
- 方法: `DELETE`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "删除成功"
}
```

##### 4.4.6 获取机柜下的网络

**请求**：
- URL: `/api/resources/cabinets/{id}/networks`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "name": "192.168.1.0/24",
      "description": "办公网络",
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

#### 4.5 工位管理

##### 4.5.1 获取工位列表

**请求**：
- URL: `/api/resources/workstations`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "name": "工位1",
      "room_id": 1,
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

##### 4.5.2 创建工位

**请求**：
- URL: `/api/resources/workstations`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 是 | 工位名称 |
| `room_id` | integer | 是 | 房间ID |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 2,
    "name": "工位2",
    "room_id": 1,
    "created_at": "2024-01-02T00:00:00"
  },
  "message": "创建成功"
}
```

##### 4.5.3 获取工位详情

**请求**：
- URL: `/api/resources/workstations/{id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "工位1",
    "room_id": 1,
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "获取成功"
}
```

##### 4.5.4 更新工位

**请求**：
- URL: `/api/resources/workstations/{id}`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 否 | 工位名称 |
| `room_id` | integer | 否 | 房间ID |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "工位1",
    "room_id": 1,
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "更新成功"
}
```

##### 4.5.5 删除工位

**请求**：
- URL: `/api/resources/workstations/{id}`
- 方法: `DELETE`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "删除成功"
}
```

#### 4.6 机位管理

##### 4.6.1 获取机位列表

**请求**：
- URL: `/api/resources/positions`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "name": "机位1",
      "cabinet_id": 1,
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

##### 4.6.2 创建机位

**请求**：
- URL: `/api/resources/positions`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 是 | 机位名称 |
| `cabinet_id` | integer | 是 | 机柜ID |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 2,
    "name": "机位2",
    "cabinet_id": 1,
    "created_at": "2024-01-02T00:00:00"
  },
  "message": "创建成功"
}
```

##### 4.6.3 获取机位详情

**请求**：
- URL: `/api/resources/positions/{id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "机位1",
    "cabinet_id": 1,
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "获取成功"
}
```

##### 4.6.4 更新机位

**请求**：
- URL: `/api/resources/positions/{id}`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 否 | 机位名称 |
| `cabinet_id` | integer | 否 | 机柜ID |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "机位1",
    "cabinet_id": 1,
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "更新成功"
}
```

##### 4.6.5 删除机位

**请求**：
- URL: `/api/resources/positions/{id}`
- 方法: `DELETE`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "删除成功"
}
```

#### 4.7 IP管理

##### 4.7.1 获取IP管理列表

**请求**：
- URL: `/api/resources/ip`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "ip_address": "192.168.1.1",
      "network_id": 1,
      "status": "assigned",
      "assigned_to": "工位1",
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

##### 4.7.2 拉取IP管理器

**请求**：
- URL: `/api/resources/ip/pull`
- 方法: `POST`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "IP管理器拉取成功"
}
```

##### 4.7.3 获取可用IP

**请求**：
- URL: `/api/resources/ip/available/{network_id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    "192.168.1.2",
    "192.168.1.3",
    "192.168.1.4"
  ],
  "message": "获取成功"
}
```

##### 4.7.4 自动分配IP

**请求**：
- URL: `/api/resources/ip/auto-assign`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `network_id` | integer | 是 | 网络ID |
| `assigned_to` | string | 是 | 分配对象 |

**响应**：

```json
{
  "success": true,
  "data": {
    "ip_address": "192.168.1.2"
  },
  "message": "IP分配成功"
}
```

##### 4.7.5 批量创建IP管理器

**请求**：
- URL: `/api/resources/ip/batch`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `ips` | array | 是 | IP地址数组 |
| `network_id` | integer | 是 | 网络ID |

**响应**：

```json
{
  "success": true,
  "message": "IP管理器批量创建成功"
}
```

##### 4.7.6 获取工位IP

**请求**：
- URL: `/api/resources/ip/workstation/{id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "ip_address": "192.168.1.1",
      "network_id": 1,
      "status": "assigned",
      "assigned_to": "工位1",
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

##### 4.7.8 获取机位IP

**请求**：
- URL: `/api/resources/ip/cabinet-position/{id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 2,
      "ip_address": "192.168.1.2",
      "network_id": 1,
      "status": "assigned",
      "assigned_to": "机位1",
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

##### 4.7.7 获取交换机IP

**请求**：
- URL: `/api/resources/ip/switch/{id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 3,
      "ip_address": "192.168.1.3",
      "network_id": 1,
      "status": "assigned",
      "assigned_to": "交换机1",
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

#### 4.8 布局管理

##### 4.8.1 保存布局

**请求**：
- URL: `/api/resources/layouts`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `room_id` | integer | 是 | 房间ID |
| `layout` | object | 是 | 布局数据 |

**响应**：

```json
{
  "success": true,
  "message": "布局保存成功"
}
```

##### 4.8.2 获取工位布局

**请求**：
- URL: `/api/resources/layouts/workstation/{room_id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "room_id": 1,
    "layout": {}
  },
  "message": "获取成功"
}
```

##### 4.8.3 删除工位布局

**请求**：
- URL: `/api/resources/layouts/workstation/{room_id}`
- 方法: `DELETE`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "布局删除成功"
}
```

##### 4.8.4 获取机位布局

**请求**：
- URL: `/api/resources/layouts/positions/{network_region_id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "network_region_id": 1,
    "layout": {}
  },
  "message": "获取成功"
}
```

##### 4.8.5 删除机位布局

**请求**：
- URL: `/api/resources/layouts/positions/{network_region_id}`
- 方法: `DELETE`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "布局删除成功"
}
```

### 5. 交换机管理 API

#### 5.1 获取交换机列表

**请求**：
- URL: `/api/switches`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "name": "交换机1",
      "ip_address": "192.168.1.100",
      "model": "Cisco Catalyst 2960",
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

#### 5.2 创建交换机

**请求**：
- URL: `/api/switches`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 是 | 交换机名称 |
| `ip_address` | string | 是 | IP地址 |
| `model` | string | 否 | 型号 |
| `snmp_community` | string | 否 | SNMP 社区字符串 |
| `snmp_version` | string | 否 | SNMP 版本 |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 2,
    "name": "交换机2",
    "ip_address": "192.168.1.101",
    "model": "Cisco Catalyst 3560",
    "created_at": "2024-01-02T00:00:00"
  },
  "message": "创建成功"
}
```

#### 5.3 获取所有交换机端口

**请求**：
- URL: `/api/switches/ports`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "switch_id": 1,
      "port_number": "Gi1/0/1",
      "status": "up",
      "description": "连接到服务器1"
    }
  ],
  "message": "获取成功"
}
```

#### 5.4 测试 SNMP 连接

**请求**：
- URL: `/api/switches/test-snmp`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `ip_address` | string | 是 | IP地址 |
| `community` | string | 是 | SNMP 社区字符串 |
| `version` | string | 是 | SNMP 版本 |

**响应**：

```json
{
  "success": true,
  "message": "SNMP 连接测试成功"
}
```

#### 5.5 获取交换机详情

**请求**：
- URL: `/api/switches/{id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "交换机1",
    "ip_address": "192.168.1.100",
    "model": "Cisco Catalyst 2960",
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "获取成功"
}
```

#### 5.6 更新交换机

**请求**：
- URL: `/api/switches/{id}`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `name` | string | 否 | 交换机名称 |
| `ip_address` | string | 否 | IP地址 |
| `model` | string | 否 | 型号 |
| `snmp_community` | string | 否 | SNMP 社区字符串 |
| `snmp_version` | string | 否 | SNMP 版本 |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "name": "交换机1",
    "ip_address": "192.168.1.100",
    "model": "Cisco Catalyst 2960",
    "created_at": "2024-01-01T00:00:00"
  },
  "message": "更新成功"
}
```

#### 5.7 删除交换机

**请求**：
- URL: `/api/switches/{id}`
- 方法: `DELETE`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "删除成功"
}
```

#### 5.8 获取交换机端口

**请求**：
- URL: `/api/switches/{id}/ports`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "switch_id": 1,
      "port_number": "Gi1/0/1",
      "status": "up",
      "description": "连接到服务器1"
    }
  ],
  "message": "获取成功"
}
```

#### 5.9 创建交换机端口

**请求**：
- URL: `/api/switches/{id}/ports`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `port_number` | string | 是 | 端口号 |
| `description` | string | 否 | 描述 |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 2,
    "switch_id": 1,
    "port_number": "Gi1/0/2",
    "status": "up",
    "description": "连接到服务器2"
  },
  "message": "创建成功"
}
```

#### 5.10 测试交换机 SNMP 连接

**请求**：
- URL: `/api/switches/{id}/test-snmp`
- 方法: `POST`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "SNMP 连接测试成功"
}
```

#### 5.11 获取交换机 MAC 表

**请求**：
- URL: `/api/switches/{id}/mac-table`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "mac_address": "00:11:22:33:44:55",
      "port": "Gi1/0/1",
      "vlan": "1"
    }
  ],
  "message": "获取成功"
}
```

#### 5.12 获取交换机 LLDP 邻居

**请求**：
- URL: `/api/switches/{id}/lldp-neighbors`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "local_port": "Gi1/0/1",
      "neighbor_device": "Server1",
      "neighbor_port": "eth0"
    }
  ],
  "message": "获取成功"
}
```

#### 5.13 获取交换机 SNMP 信息

**请求**：
- URL: `/api/switches/{id}/snmp-info`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "sysName": "Switch1",
    "sysDescr": "Cisco Catalyst 2960 Series Switch",
    "sysUpTime": "123456789"
  },
  "message": "获取成功"
}
```

#### 5.14 获取交换机 SNMP 端口

**请求**：
- URL: `/api/switches/{id}/snmp-ports`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "port": "Gi1/0/1",
      "status": "up",
      "description": "连接到服务器1"
    }
  ],
  "message": "获取成功"
}
```

#### 5.15 获取交换机端口详情

**请求**：
- URL: `/api/switches/ports/{port_id}`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "switch_id": 1,
    "port_number": "Gi1/0/1",
    "status": "up",
    "description": "连接到服务器1"
  },
  "message": "获取成功"
}
```

#### 5.16 更新交换机端口

**请求**：
- URL: `/api/switches/ports/{port_id}`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `description` | string | 否 | 描述 |
| `status` | string | 否 | 状态 |

**响应**：

```json
{
  "success": true,
  "data": {
    "id": 1,
    "switch_id": 1,
    "port_number": "Gi1/0/1",
    "status": "up",
    "description": "更新后的描述"
  },
  "message": "更新成功"
}
```

#### 5.17 删除交换机端口

**请求**：
- URL: `/api/switches/ports/{port_id}`
- 方法: `DELETE`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "删除成功"
}
```

### 6. 日志管理 API

#### 6.1 获取操作日志

**请求**：
- URL: `/api/logs/operation`
- 方法: `GET`
- 认证: 需要

**查询参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `page` | integer | 否 | 页码 |
| `page_size` | integer | 否 | 每页条数 |
| `start_time` | string | 否 | 开始时间 |
| `end_time` | string | 否 | 结束时间 |

**响应**：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "id": 1,
        "user_id": 1,
        "username": "admin",
        "action": "create_user",
        "target": "用户",
        "details": "创建了用户 user1",
        "ip_address": "192.168.1.1",
        "created_at": "2024-01-01T00:00:00"
      }
    ],
    "total": 1,
    "page": 1,
    "page_size": 10
  },
  "message": "获取成功"
}
```

#### 6.2 获取登录日志

**请求**：
- URL: `/api/logs/login`
- 方法: `GET`
- 认证: 需要

**查询参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `page` | integer | 否 | 页码 |
| `page_size` | integer | 否 | 每页条数 |
| `start_time` | string | 否 | 开始时间 |
| `end_time` | string | 否 | 结束时间 |

**响应**：

```json
{
  "success": true,
  "data": {
    "items": [
      {
        "id": 1,
        "user_id": 1,
        "username": "admin",
        "ip_address": "192.168.1.1",
        "status": "success",
        "message": "登录成功",
        "created_at": "2024-01-01T00:00:00"
      }
    ],
    "total": 1,
    "page": 1,
    "page_size": 10
  },
  "message": "获取成功"
}
```

### 7. 通知管理 API

#### 7.1 获取通知列表

**请求**：
- URL: `/api/notifications`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "user_id": 1,
      "title": "系统通知",
      "content": "系统已更新到最新版本",
      "is_read": false,
      "created_at": "2024-01-01T00:00:00"
    }
  ],
  "message": "获取成功"
}
```

#### 7.2 标记通知为已读

**请求**：
- URL: `/api/notifications/{id}/read`
- 方法: `PUT`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "标记成功"
}
```

#### 7.3 标记所有通知为已读

**请求**：
- URL: `/api/notifications/mark-all-read`
- 方法: `PUT`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "标记成功"
}
```

### 8. 系统管理 API

#### 8.1 获取系统信息

**请求**：
- URL: `/api/system/info`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "version": "0.7.2",
    "os": "Linux",
    "cpu": "Intel(R) Core(TM) i7-8700K",
    "memory": "16GB",
    "disk": "500GB",
    "uptime": "24h"
  },
  "message": "获取成功"
}
```

#### 8.2 获取仪表盘统计

**请求**：
- URL: `/api/system/dashboard-stats`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "total_users": 10,
    "total_networks": 5,
    "total_ips": 256,
    "used_ips": 128,
    "total_switches": 3
  },
  "message": "获取成功"
}
```

#### 8.3 检查服务状态

**请求**：
- URL: `/api/system/service-status`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "database": "online",
    "smtp": "online",
    "snmp": "online"
  },
  "message": "获取成功"
}
```

#### 8.4 重启应用系统

**请求**：
- URL: `/api/system/restart-application`
- 方法: `POST`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "应用系统重启中"
}
```

#### 8.5 重启操作系统

**请求**：
- URL: `/api/system/restart-os`
- 方法: `POST`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "操作系统重启中"
}
```

#### 8.6 注册为服务

**请求**：
- URL: `/api/system/register-service`
- 方法: `POST`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "服务注册成功"
}
```

#### 8.7 关闭初始化模式

**请求**：
- URL: `/api/system/disable-init`
- 方法: `POST`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "初始化模式已关闭"
}
```

#### 8.8 SMTP 配置

##### 8.8.1 获取 SMTP 配置

**请求**：
- URL: `/api/system/smtp/config`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "host": "smtp.example.com",
    "port": 587,
    "username": "admin@example.com",
    "password": "********",
    "from": "admin@example.com",
    "tls": true
  },
  "message": "获取成功"
}
```

##### 8.8.2 更新 SMTP 配置

**请求**：
- URL: `/api/system/smtp/config`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `host` | string | 是 | SMTP 服务器地址 |
| `port` | integer | 是 | SMTP 端口 |
| `username` | string | 是 | 用户名 |
| `password` | string | 是 | 密码 |
| `from` | string | 是 | 发件人邮箱 |
| `tls` | boolean | 是 | 是否使用 TLS |

**响应**：

```json
{
  "success": true,
  "message": "SMTP 配置更新成功"
}
```

##### 8.8.3 测试 SMTP 连接

**请求**：
- URL: `/api/system/smtp/test`
- 方法: `POST`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "SMTP 连接测试成功"
}
```

##### 8.8.4 发送邮件

**请求**：
- URL: `/api/system/smtp/send`
- 方法: `POST`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `to` | string | 是 | 收件人邮箱 |
| `subject` | string | 是 | 邮件主题 |
| `body` | string | 是 | 邮件内容 |

**响应**：

```json
{
  "success": true,
  "message": "邮件发送成功"
}
```

#### 8.9 证书管理

##### 8.9.1 获取证书状态

**请求**：
- URL: `/api/system/certificate/status`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "status": "valid",
    "expires_at": "2025-01-01T00:00:00"
  },
  "message": "获取成功"
}
```

##### 8.9.2 生成证书

**请求**：
- URL: `/api/system/certificate/generate`
- 方法: `POST`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "证书生成成功"
}
```

##### 8.9.3 导入证书

**请求**：
- URL: `/api/system/certificate/import`
- 方法: `POST`
- 内容类型: `multipart/form-data`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `certificate` | file | 是 | 证书文件 |
| `private_key` | file | 是 | 私钥文件 |

**响应**：

```json
{
  "success": true,
  "message": "证书导入成功"
}
```

##### 8.9.4 下载证书

**请求**：
- URL: `/api/system/certificate/download`
- 方法: `GET`
- 认证: 需要

**响应**：

证书文件下载

#### 8.10 配置管理

##### 8.10.1 更新系统配置

**请求**：
- URL: `/api/system/config`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `key` | string | 是 | 配置键 |
| `value` | any | 是 | 配置值 |

**响应**：

```json
{
  "success": true,
  "message": "配置更新成功"
}
```

##### 8.10.2 备份配置

**请求**：
- URL: `/api/system/config/backup`
- 方法: `GET`
- 认证: 需要

**响应**：

配置文件下载

##### 8.10.3 恢复配置

**请求**：
- URL: `/api/system/config/restore`
- 方法: `POST`
- 内容类型: `multipart/form-data`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `config_file` | file | 是 | 配置文件 |

**响应**：

```json
{
  "success": true,
  "message": "配置恢复成功"
}
```

#### 8.11 语言设置

##### 8.11.1 获取支持的语言

**请求**：
- URL: `/api/system/languages`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": [
    "zh-CN",
    "en"
  ],
  "message": "获取成功"
}
```

##### 8.11.2 更新语言设置

**请求**：
- URL: `/api/system/language`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `language` | string | 是 | 语言代码 |

**响应**：

```json
{
  "success": true,
  "message": "语言设置更新成功"
}
```

#### 8.12 页面超时配置

##### 8.12.1 获取页面超时配置

**请求**：
- URL: `/api/system/page-timeout`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "timeout": 30
  },
  "message": "获取成功"
}
```

##### 8.12.2 更新页面超时配置

**请求**：
- URL: `/api/system/page-timeout`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `timeout` | integer | 是 | 超时时间（分钟） |

**响应**：

```json
{
  "success": true,
  "message": "页面超时配置更新成功"
}
```

#### 8.13 会话超时配置

##### 8.13.1 获取会话超时配置

**请求**：
- URL: `/api/system/session-timeout`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "timeout": 120
  },
  "message": "获取成功"
}
```

##### 8.13.2 更新会话超时配置

**请求**：
- URL: `/api/system/session-timeout`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `timeout` | integer | 是 | 超时时间（分钟） |

**响应**：

```json
{
  "success": true,
  "message": "会话超时配置更新成功"
}
```

#### 8.14 通知设置

##### 8.14.1 获取通知设置

**请求**：
- URL: `/api/system/notification/settings`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "email_notifications": true,
    "system_notifications": true
  },
  "message": "获取成功"
}
```

##### 8.14.2 更新通知设置

**请求**：
- URL: `/api/system/notification/settings`
- 方法: `PUT`
- 内容类型: `application/json`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `email_notifications` | boolean | 是 | 是否启用邮件通知 |
| `system_notifications` | boolean | 是 | 是否启用系统通知 |

**响应**：

```json
{
  "success": true,
  "message": "通知设置更新成功"
}
```

#### 8.15 导入导出功能

##### 8.15.1 导入 CSV

**请求**：
- URL: `/api/system/import-export/import/csv`
- 方法: `POST`
- 内容类型: `multipart/form-data`
- 认证: 需要

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `file` | file | 是 | CSV 文件 |
| `type` | string | 是 | 导入类型 |

**响应**：

```json
{
  "success": true,
  "message": "CSV 导入成功"
}
```

##### 8.15.2 导出 CSV

**请求**：
- URL: `/api/system/import-export/export/csv`
- 方法: `GET`
- 认证: 需要

**查询参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `type` | string | 是 | 导出类型 |

**响应**：

CSV 文件下载

##### 8.15.3 导出数据库

**请求**：
- URL: `/api/system/import-export/export/database`
- 方法: `GET`
- 认证: 需要

**响应**：

数据库备份文件下载

##### 8.15.4 下载模板

**请求**：
- URL: `/api/system/import-export/template`
- 方法: `GET`
- 认证: 需要

**查询参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `type` | string | 是 | 模板类型 |

**响应**：

模板文件下载

#### 8.16 日志清理功能

##### 8.16.1 获取日志统计

**请求**：
- URL: `/api/system/logs/stats`
- 方法: `GET`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "data": {
    "operation_logs": 1000,
    "login_logs": 500,
    "total_size": "10MB"
  },
  "message": "获取成功"
}
```

##### 8.16.2 清理日志

**请求**：
- URL: `/api/system/logs/clear`
- 方法: `POST`
- 认证: 需要

**响应**：

```json
{
  "success": true,
  "message": "日志清理成功"
}
```

## 9. 健康检查 API

#### 9.1 健康检查

**请求**：
- URL: `/health`
- 方法: `GET`

**响应**：

```json
{
  "status": "ok"
}
```

## 10. 初始化 API

#### 10.1 初始化系统

**请求**：
- URL: `/api/init`
- 方法: `POST`
- 内容类型: `application/json`

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `admin_username` | string | 是 | 管理员用户名 |
| `admin_email` | string | 是 | 管理员邮箱 |
| `admin_password` | string | 是 | 管理员密码 |
| `database_url` | string | 是 | 数据库连接 URL |

**响应**：

```json
{
  "success": true,
  "message": "系统初始化成功"
}
```

#### 10.2 初始化数据库

**请求**：
- URL: `/api/init/db`
- 方法: `POST`
- 内容类型: `application/json`

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `database_url` | string | 是 | 数据库连接 URL |

**响应**：

```json
{
  "success": true,
  "message": "数据库初始化成功"
}
```

#### 10.3 清空数据库

**请求**：
- URL: `/api/init/db/clear`
- 方法: `DELETE`

**响应**：

```json
{
  "success": true,
  "message": "数据库清空成功"
}
```

#### 10.4 创建数据库

**请求**：
- URL: `/api/init/db/create`
- 方法: `POST`
- 内容类型: `application/json`

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `database_name` | string | 是 | 数据库名称 |
| `username` | string | 是 | 数据库用户名 |
| `password` | string | 是 | 数据库密码 |

**响应**：

```json
{
  "success": true,
  "message": "数据库创建成功"
}
```

#### 10.5 导入数据库

**请求**：
- URL: `/api/init/db/import`
- 方法: `POST`
- 内容类型: `application/json`

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `database_url` | string | 是 | 数据库连接 URL |
| `sql_content` | string | 是 | SQL 内容 |

**响应**：

```json
{
  "success": true,
  "message": "数据库导入成功"
}
```

#### 10.6 从文件导入数据库

**请求**：
- URL: `/api/init/db/import-file`
- 方法: `POST`
- 内容类型: `multipart/form-data`

**请求参数**：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `file` | file | 是 | SQL 文件 |
| `database_url` | string | 是 | 数据库连接 URL |

**响应**：

```json
{
  "success": true,
  "message": "数据库导入成功"
}
```

#### 10.7 重启程序

**请求**：
- URL: `/api/init/restart`
- 方法: `POST`

**响应**：

```json
{
  "success": true,
  "message": "程序重启中"
}
```

#### 10.8 检查初始化状态

**请求**：
- URL: `/api/init/status`
- 方法: `GET`

**响应**：

```json
{
  "success": true,
  "data": {
    "initialized": true,
    "database_connected": true
  },
  "message": "获取成功"
}
```

#### 10.9 检查数据库状态

**请求**：
- URL: `/api/init/db-status`
- 方法: `GET`

**响应**：

```json
{
  "success": true,
  "data": {
    "connected": true,
    "version": "PostgreSQL 14.0"
  },
  "message": "获取成功"
}
```

#### 10.10 获取验证码

**请求**：
- URL: `/api/init/verification-code`
- 方法: `GET`

**响应**：

```json
{
  "success": true,
  "data": {
    "code": "123456"
  },
  "message": "获取成功"
}
```

#### 10.11 检查 PostgreSQL 连接

**请求**：
- URL: `/api/init/check-pgsql`
- 方法: `GET`

**响应**：

```json
{
  "success": true,
  "data": {
    "available": true,
    "version": "PostgreSQL 14.0"
  },
  "message": "获取成功"
}
```

## 总结

本 API 文档详细描述了 IPMA 系统的所有 API 接口，包括认证、用户管理、资源管理、交换机管理、日志管理、通知管理、系统管理、健康检查和初始化等功能。

所有 API 接口都遵循统一的响应格式，使用 HttpOnly Cookie 进行认证，确保系统的安全性。

开发者在使用 API 时应注意：
1. 所有需要认证的 API 都需要在请求中设置 `credentials: 'include'`
2. 所有请求和响应都遵循 JSON 格式
3. 错误处理应根据响应中的 `success` 字段和 `error_type` 字段进行
4. 对于文件上传和下载，应使用 `multipart/form-data` 格式或处理二进制数据

本文档将根据系统的更新和功能扩展进行定期更新。