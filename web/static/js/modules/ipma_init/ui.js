/**
 * IPMA 系统初始化 - DOM 交互 / 视图层
 *
 * 只负责页面元素的显示、隐藏、切换与状态渲染，不直接发起网络请求。
 * 依赖共享状态 state（来自 ./state.js）。
 */
import { state } from './state.js';

/**
 * 切换到指定步骤：显示对应面板，并更新顶部步骤指示器的 active / completed 标记。
 */
export const goToStep = (stepNum) => {
    document.querySelectorAll('.step-panel').forEach(panel => {
        panel.classList.remove('active');
    });
    document.querySelector(`.step-panel[data-step="${stepNum}"]`).classList.add('active');
    document.querySelectorAll('.step').forEach(step => {
        const stepData = parseInt(step.dataset.step);
        if (stepData < stepNum) {
            step.classList.add('completed');
            step.classList.remove('active');
        } else if (stepData === stepNum) {
            step.classList.add('active');
            step.classList.remove('completed');
        } else {
            step.classList.remove('active', 'completed');
        }
    });
    state.currentStep = stepNum;
};

/**
 * 在右上角短暂显示错误提示，5 秒后自动隐藏。
 */
export const showError = (message) => {
    const errorElement = document.getElementById('error-message');
    errorElement.textContent = message;
    errorElement.style.display = 'block';
    setTimeout(() => {
        errorElement.style.display = 'none';
    }, 5000);
};

export const showLoading = () => {
    document.getElementById('loading').style.display = 'flex';
};

export const hideLoading = () => {
    document.getElementById('loading').style.display = 'none';
};

/**
 * 切换数据库初始化方式（新建 / 导入）。
 * 由 init_index.html 的 inline onclick 调用，因此会被 index.js 挂到 window。
 */
export const setInitMode = (mode) => {
    state.initMode = mode;
    document.querySelectorAll('.init-tab').forEach(tab => {
        tab.classList.remove('active');
    });
    document.querySelector(`.init-tab[data-mode="${mode}"]`).classList.add('active');

    const createPanel = document.getElementById('create-panel');
    const importPanel = document.getElementById('import-panel');

    if (mode === 'create') {
        createPanel.style.display = 'block';
        importPanel.style.display = 'none';
    } else {
        createPanel.style.display = 'none';
        importPanel.style.display = 'block';
    }
};
