export const ValidationRules = {
  ip: {
    pattern: /^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$/,
    patternV6: /^(?:[0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}$|^::(?:[0-9a-fA-F]{1,4}:){0,6}[0-9a-fA-F]{1,4}$|^(?:[0-9a-fA-F]{1,4}:){1,7}:$|^(?:[0-9a-fA-F]{1,4}:){0,6}::(?:[0-9a-fA-F]{1,4}:){0,5}[0-9a-fA-F]{1,4}$/,
    message: "请输入有效的IP地址"
  },
  mac: {
    pattern: /^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$|^([0-9A-Fa-f]{4}[.:-]){2}([0-9A-Fa-f]{4})$|^[0-9A-Fa-f]{12}$/,
    message: "请输入有效的MAC地址（如: 00:11:22:33:44:55）"
  },
  hostname: {
    pattern: /^[a-zA-Z0-9]([a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?)*$/,
    message: "请输入有效的主机名（只允许字母、数字、连字符和点）"
  },
  email: {
    pattern: /^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$/,
    message: "请输入有效的邮箱地址"
  },
  phone: {
    pattern: /^1[3-9]\d{9}$/,
    message: "请输入有效的手机号码"
  },
  username: {
    pattern: /^[a-zA-Z][a-zA-Z0-9_]{2,31}$/,
    message: "用户名必须以字母开头，3-32个字符，只能包含字母、数字和下划线"
  },
  password: {
    minLength: 8,
    maxLength: 128,
    requireUppercase: true,
    requireLowercase: true,
    requireNumber: true,
    requireSpecial: false,
    message: "密码长度8-128位，必须包含大小写字母和数字"
  },
  cidr: {
    pattern: /^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\/(?:3[0-2]|[12]?[0-9])$/,
    message: "请输入有效的CIDR格式（如: 192.168.1.0/24）"
  },
  port: {
    min: 1,
    max: 65535,
    message: "端口号必须在1-65535之间"
  },
  vlan: {
    min: 1,
    max: 4094,
    message: "VLAN ID必须在1-4094之间"
  }
};

export function validateIp(value) {
  if (!value) return { valid: true };
  const isIPv4 = ValidationRules.ip.pattern.test(value);
  const isIPv6 = ValidationRules.ip.patternV6.test(value);
  if (!isIPv4 && !isIPv6) {
    return { valid: false, message: ValidationRules.ip.message };
  }
  return { valid: true };
}

export function validateMac(value) {
  if (!value) return { valid: true };
  if (!ValidationRules.mac.pattern.test(value)) {
    return { valid: false, message: ValidationRules.mac.message };
  }
  return { valid: true };
}

export function validateHostname(value) {
  if (!value) return { valid: true };
  if (value.length > 100) {
    return { valid: false, message: "主机名长度不能超过100个字符" };
  }
  if (!ValidationRules.hostname.pattern.test(value)) {
    return { valid: false, message: ValidationRules.hostname.message };
  }
  return { valid: true };
}

export function validateEmail(value) {
  if (!value) return { valid: true };
  if (!ValidationRules.email.pattern.test(value)) {
    return { valid: false, message: ValidationRules.email.message };
  }
  return { valid: true };
}

export function validatePassword(value) {
  if (!value) return { valid: false, message: "密码不能为空" };
  const rules = ValidationRules.password;
  const errors = [];
  
  if (value.length < rules.minLength) {
    errors.push(`密码长度至少${rules.minLength}位`);
  }
  if (value.length > rules.maxLength) {
    errors.push(`密码长度不能超过${rules.maxLength}位`);
  }
  if (rules.requireUppercase && !/[A-Z]/.test(value)) {
    errors.push("密码必须包含大写字母");
  }
  if (rules.requireLowercase && !/[a-z]/.test(value)) {
    errors.push("密码必须包含小写字母");
  }
  if (rules.requireNumber && !/[0-9]/.test(value)) {
    errors.push("密码必须包含数字");
  }
  if (rules.requireSpecial && !/[!@#$%^&*(),.?":{}|<>]/.test(value)) {
    errors.push("密码必须包含特殊字符");
  }
  
  if (errors.length > 0) {
    return { valid: false, message: errors.join("，") };
  }
  return { valid: true };
}

export function validateUsername(value) {
  if (!value) return { valid: false, message: "用户名不能为空" };
  if (!ValidationRules.username.pattern.test(value)) {
    return { valid: false, message: ValidationRules.username.message };
  }
  return { valid: true };
}

export function validateCidr(value) {
  if (!value) return { valid: true };
  if (!ValidationRules.cidr.pattern.test(value)) {
    return { valid: false, message: ValidationRules.cidr.message };
  }
  return { valid: true };
}

export function validatePort(value) {
  if (!value) return { valid: true };
  const port = parseInt(value, 10);
  if (isNaN(port) || port < ValidationRules.port.min || port > ValidationRules.port.max) {
    return { valid: false, message: ValidationRules.port.message };
  }
  return { valid: true };
}

export function validateVlan(value) {
  if (!value) return { valid: true };
  const vlan = parseInt(value, 10);
  if (isNaN(vlan) || vlan < ValidationRules.vlan.min || vlan > ValidationRules.vlan.max) {
    return { valid: false, message: ValidationRules.vlan.message };
  }
  return { valid: true };
}

export function validateRequired(value, fieldName = "此字段") {
  if (value === null || value === undefined || value === "" || (Array.isArray(value) && value.length === 0)) {
    return { valid: false, message: `${fieldName}不能为空` };
  }
  return { valid: true };
}

export function validateLength(value, min, max, fieldName = "内容") {
  if (!value) return { valid: true };
  if (value.length < min) {
    return { valid: false, message: `${fieldName}长度不能少于${min}个字符` };
  }
  if (value.length > max) {
    return { valid: false, message: `${fieldName}长度不能超过${max}个字符` };
  }
  return { valid: true };
}

export function validateIpInNetwork(ip, cidr) {
  if (!ip || !cidr) return { valid: true };
  
  try {
    const [network, prefix] = cidr.split('/');
    const prefixNum = parseInt(prefix, 10);
    
    const ipNum = ip.split('.').reduce((acc, octet) => (acc << 8) + parseInt(octet, 10), 0);
    const networkNum = network.split('.').reduce((acc, octet) => (acc << 8) + parseInt(octet, 10), 0);
    const mask = (0xFFFFFFFF << (32 - prefixNum)) >>> 0;
    
    if ((ipNum & mask) !== (networkNum & mask)) {
      return { valid: false, message: `IP地址 ${ip} 不在网段 ${cidr} 范围内` };
    }
    return { valid: true };
  } catch (e) {
    return { valid: false, message: "IP地址或网段格式错误" };
  }
}

export function validateForm(formData, rules) {
  const errors = [];
  
  for (const [field, fieldRules] of Object.entries(rules)) {
    const value = formData[field];
    
    for (const rule of fieldRules) {
      let result = { valid: true };
      
      switch (rule.type) {
        case 'required':
          result = validateRequired(value, rule.message || field);
          break;
        case 'ip':
          result = validateIp(value);
          break;
        case 'mac':
          result = validateMac(value);
          break;
        case 'hostname':
          result = validateHostname(value);
          break;
        case 'email':
          result = validateEmail(value);
          break;
        case 'password':
          result = validatePassword(value);
          break;
        case 'username':
          result = validateUsername(value);
          break;
        case 'cidr':
          result = validateCidr(value);
          break;
        case 'port':
          result = validatePort(value);
          break;
        case 'vlan':
          result = validateVlan(value);
          break;
        case 'length':
          result = validateLength(value, rule.min, rule.max, rule.fieldName || field);
          break;
        case 'custom':
          if (rule.validator) {
            result = rule.validator(value, formData);
          }
          break;
      }
      
      if (!result.valid) {
        errors.push({ field, message: result.message });
        break;
      }
    }
  }
  
  return {
    valid: errors.length === 0,
    errors
  };
}

export function showValidationErrors(errors, prefix = '') {
  errors.forEach(error => {
    const field = document.getElementById(prefix + error.field);
    if (field) {
      field.classList.add('error');
      const errorEl = field.parentElement.querySelector('.error-text');
      if (errorEl) {
        errorEl.textContent = error.message;
        errorEl.style.display = 'block';
      }
    }
  });
}

export function clearValidationErrors(prefix = '') {
  document.querySelectorAll('.error').forEach(el => el.classList.remove('error'));
  document.querySelectorAll('.error-text').forEach(el => {
    el.textContent = '';
    el.style.display = 'none';
  });
}

export function normalizeMacAddress(mac) {
  if (!mac) return '';
  return mac.toUpperCase().replace(/[^0-9A-F]/g, '').match(/.{2}/g)?.join(':') || '';
}

export function formatMacAddress(mac) {
  if (!mac) return '';
  const normalized = normalizeMacAddress(mac);
  return normalized;
}
