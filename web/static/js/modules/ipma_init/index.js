/**
 * IPMA 系统初始化 - 入口模块
 *
 * 由 init_index.html 以 <script type="module"> 加载。
 * 负责在 DOM 就绪后绑定事件、暴露 inline onclick 所需的全局函数，并触发首次检查。
 */
import { state } from './state.js';
import { goToStep, setInitMode } from './ui.js';
import {
    checkPostgreSQL,
    checkDatabaseStatus,
    handleInitModeSubmit,
    handleAdminAccountSubmit,
    getVerificationCode,
} from './api.js';

// init_index.html 使用 inline onclick 调用以下两个函数，需挂到 window。
window.getVerificationCode = getVerificationCode;
window.setInitMode = setInitMode;

document.addEventListener('DOMContentLoaded', async () => {
    document.getElementById('init-mode-form')?.addEventListener('submit', handleInitModeSubmit);
    document.getElementById('admin-account-form')?.addEventListener('submit', handleAdminAccountSubmit);

    document.querySelectorAll('.prev-step').forEach(button => {
        button.addEventListener('click', () => {
            goToStep(state.currentStep - 1);
        });
    });

    document.querySelectorAll('.step').forEach(step => {
        step.addEventListener('click', () => {
            const stepNum = parseInt(step.dataset.step);
            if (stepNum < state.currentStep) {
                goToStep(stepNum);
            }
        });
    });

    document.querySelector('.step-panel[data-step="1"] .btn-success')?.addEventListener('click', () => {
        goToStep(2);
        checkDatabaseStatus();
    });

    await checkPostgreSQL();
});
