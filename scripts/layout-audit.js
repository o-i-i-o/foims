#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

const cssDir = './web/static/css';
const issues = [];

function checkCssFile(filePath) {
    const content = fs.readFileSync(filePath, 'utf8');
    const lines = content.split('\n');
    
    let currentSelector = '';
    
    lines.forEach((line, index) => {
        const lineNum = index + 1;
        
        if (line.match(/^[^{]*\{/) && !line.match(/^\s*\*/)) {
            const match = line.match(/([.#]?[\w-]+(?:\s*[,+>]\s*[.#]?[\w-]+)*)\s*\{/);
            if (match) {
                currentSelector = match[1].trim();
            }
        }
        
        if (line.includes('}') && !line.includes('{')) {
            currentSelector = '';
        }
        
        if (line.includes('!important') && !line.includes('/* allowed */')) {
            if (currentSelector === '.hidden' || currentSelector.includes('.hidden')) {
                return;
            }
            issues.push({
                file: filePath,
                line: lineNum,
                type: '!important',
                message: '使用 !important 可能导致样式覆盖问题'
            });
        }
        
        if (line.match(/margin:\s*\d+px\s+\d+px\s+\d+px\s+\d+px/)) {
            issues.push({
                file: filePath,
                line: lineNum,
                type: 'margin',
                message: '建议使用 margin-top/right/bottom/left 分开声明以提高可读性'
            });
        }
        
        if (line.match(/padding:\s*\d+px\s+\d+px\s+\d+px\s+\d+px/)) {
            issues.push({
                file: filePath,
                line: lineNum,
                type: 'padding',
                message: '建议使用 padding-top/right/bottom/left 分开声明以提高可读性'
            });
        }
        
        if (line.match(/font-size:\s*\d+px/) && !line.match(/font-size:\s*(12|14|16|18|20|24|28|32)px/)) {
            issues.push({
                file: filePath,
                line: lineNum,
                type: 'font-size',
                message: '字体大小建议使用标准值 (12, 14, 16, 18, 20, 24, 28, 32px)'
            });
        }
        
        if (line.match(/z-index:\s*\d+/)) {
            const zValue = parseInt(line.match(/z-index:\s*(\d+)/)[1]);
            if (zValue > 1000) {
                issues.push({
                    file: filePath,
                    line: lineNum,
                    type: 'z-index',
                    message: `z-index 值 ${zValue} 过大，可能导致层级管理混乱`
                });
            }
        }
    });
}

function walkDir(dir) {
    const files = fs.readdirSync(dir);
    files.forEach(file => {
        const filePath = path.join(dir, file);
        const stat = fs.statSync(filePath);
        if (stat.isDirectory()) {
            walkDir(filePath);
        } else if (file.endsWith('.css')) {
            checkCssFile(filePath);
        }
    });
}

console.log('=== CSS 布局审计报告 ===\n');

try {
    walkDir(cssDir);
    
    if (issues.length === 0) {
        console.log('未发现明显的 CSS 布局问题。\n');
    } else {
        const grouped = {};
        issues.forEach(issue => {
            if (!grouped[issue.type]) grouped[issue.type] = [];
            grouped[issue.type].push(issue);
        });
        
        Object.entries(grouped).forEach(([type, items]) => {
            console.log(`\n【${type} 问题】 (${items.length} 处)`);
            items.slice(0, 5).forEach(item => {
                console.log(`  ${path.relative('.', item.file)}:${item.line}`);
                console.log(`    ${item.message}`);
            });
            if (items.length > 5) {
                console.log(`  ... 还有 ${items.length - 5} 处`);
            }
        });
        
        console.log(`\n总计: ${issues.length} 个潜在问题`);
    }
} catch (e) {
    console.log('无法扫描 CSS 目录:', e.message);
}

console.log('\n=== HTML 结构检查 ===\n');

const htmlFile = './web/static/main.html';
if (fs.existsSync(htmlFile)) {
    const html = fs.readFileSync(htmlFile, 'utf8');
    
    const checks = [
        { name: 'DOCTYPE', pattern: /<!DOCTYPE html>/, required: true },
        { name: 'viewport meta', pattern: /<meta name="viewport"/, required: true },
        { name: 'charset meta', pattern: /<meta charset=/, required: true },
        { name: 'lang attribute', pattern: /<html lang=/, required: true },
        { name: 'title tag', pattern: /<title>.*<\/title>/, required: true },
        { name: 'main heading', pattern: /<h1/, required: false },
        { name: 'skip link', pattern: /skip.*content|跳转.*内容/, required: false, message: '缺少跳转链接（无障碍访问）' },
        { name: 'aria labels', pattern: /aria-label=/, required: false, message: '建议添加更多 aria-label 属性' },
    ];
    
    checks.forEach(check => {
        const found = check.pattern.test(html);
        if (check.required && !found) {
            console.log(`❌ 缺少: ${check.name}`);
        } else if (!check.required && !found) {
            console.log(`⚠️  建议: ${check.message || check.name}`);
        } else {
            console.log(`✓ ${check.name}`);
        }
    });
}

console.log('\n=== 完成 ===');
