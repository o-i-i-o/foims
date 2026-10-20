<script setup>
/**
 * 线缆标签打印区（移植自旧版 cableLink.js printCableLabels）：
 * 25x38+40 规格——标签 25mm x 38mm、步进 40mm，一枚标签独占一页；
 * 每条线路打印两枚（A端+B端 与 B端+A端，本端在前、对端在后）。
 * 挂载后立即调起浏览器打印，打印结束/取消（afterprint）后通知父组件移除；
 * 打印遮蔽规则见 styles/components/cable-label-print.css。
 */
import { onBeforeUnmount, onMounted } from "vue";

const props = defineProps({
  /** 待打印线路（a_endpoint_label/b_endpoint_label） */
  items: { type: Array, required: true }
});

const emit = defineEmits(["done"]);

function onAfterPrint() {
  emit("done");
}

onMounted(() => {
  window.addEventListener("afterprint", onAfterPrint, { once: true });
  // 等待 Teleport 内容完成一次渲染后再调起打印
  requestAnimationFrame(() => window.print());
});

onBeforeUnmount(() => {
  window.removeEventListener("afterprint", onAfterPrint);
});
</script>

<template>
  <Teleport to="body">
    <div class="cable-label-print-area">
      <template v-for="(item, index) in props.items" :key="index">
        <!-- 一条线路两枚标签：A端+B端 与 B端+A端（本端在前、对端在后） -->
        <div class="cable-label">
          <div class="cable-label-text">{{ item.a_endpoint_label || "-" }}</div>
          <div class="cable-label-divider"></div>
          <div class="cable-label-text">{{ item.b_endpoint_label || "-" }}</div>
        </div>
        <div class="cable-label">
          <div class="cable-label-text">{{ item.b_endpoint_label || "-" }}</div>
          <div class="cable-label-divider"></div>
          <div class="cable-label-text">{{ item.a_endpoint_label || "-" }}</div>
        </div>
      </template>
    </div>
  </Teleport>
</template>
