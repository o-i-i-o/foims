import{t,b as g,e as N,f as u,d as H}from"./authManager-B4KDTqy2-DSJgf8nQ.js";async function F(a){const e=document.createElement("div");e.className="modal active",e.id="arp-modal-"+Date.now(),e.innerHTML=`
    <div class="modal-content" style="max-width: 800px;">
      <div class="modal-header">
        <h3>${t("switch.mac_table")||"MAC表"} <span id="arp-switch-name"></span></h3>
        <span class="close arp-modal-close">&times;</span>
      </div>
      <div class="modal-body" style="max-height: 500px; overflow-y: auto;">
        <div id="arp-loading" style="text-align: center; padding: 40px;">
          <div class="spinner"></div>
          <p style="margin-top: 10px; color: #666;">${t("switch.loading_mac")||"正在加载MAC表..."}</p>
        </div>
        <div id="arp-content" style="display: none;">
          <div class="tab-container">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;">
              <div class="tab-buttons" style="display: flex; gap: 5px;">
                <button class="tab-btn active" data-tab="ipv4">IPv4</button>
                <button class="tab-btn" data-tab="ipv6">IPv6</button>
              </div>
              <button class="btn btn-sm btn-primary" id="sync-mac-btn">
                <span>${t("switch.sync_from_snmp")||"从SNMP同步"}</span>
              </button>
            </div>
            <div class="tab-content active" id="ipv4-tab">
              <div style="margin-bottom: 10px;">
                <input type="text" id="ipv4-search" placeholder="${t("switch.search_ip_mac")||"搜索IP或MAC地址..."}" style="width: 100%; padding: 8px; border: 1px solid #ccc; border-radius: 4px;">
              </div>
              <div id="ipv4-table-container" style="max-height: 350px; overflow-y: auto;"></div>
            </div>
            <div class="tab-content" id="ipv6-tab">
              <div style="margin-bottom: 10px;">
                <input type="text" id="ipv6-search" placeholder="${t("switch.search_ip_mac")||"搜索IP或MAC地址..."}" style="width: 100%; padding: 8px; border: 1px solid #ccc; border-radius: 4px;">
              </div>
              <div id="ipv6-table-container" style="max-height: 350px; overflow-y: auto;"></div>
            </div>
          </div>
        </div>
      </div>
      <div class="modal-footer">
        <button class="btn btn-secondary arp-modal-close">${t("common.close")||"关闭"}</button>
      </div>
    </div>
  `,document.body.appendChild(e),e.querySelectorAll(".arp-modal-close").forEach(n=>{n.addEventListener("click",()=>e.remove())}),e.addEventListener("click",n=>{n.target===e&&e.remove()});let o=!0,i=null,d=null,r=[],s=[];const l=async()=>{const n=e.querySelector("#arp-loading"),b=e.querySelector("#arp-content"),y=e.querySelector("#arp-switch-name"),p=e.querySelector("#sync-mac-btn");n&&(n.style.display="block"),b&&(b.style.display="none"),p&&(p.disabled=!0);try{const m=await g(`/api/switches/${a}`);if(!m.success){n&&(n.innerHTML=`<p style="color: red;">${t("switch.fetch_info_failed")||"获取交换机信息失败"}</p>`);return}const w=m.data;y&&(y.textContent=`- ${w.name}`);const q=await g(`/api/switches/${a}/macs`);if(q.success){const x=q.data||[];if(y&&(y.textContent=`- ${w.name} (${x.length}${t("switch.entries")||"条"})`),x.length===0){n&&(n.innerHTML=`
              <p style="color: #666; margin-bottom: 10px;">${t("switch.no_mac_data")||'暂无MAC数据，请点击"从SNMP同步"按钮获取'}</p>
              <button class="btn btn-primary" id="sync-mac-empty-btn">${t("switch.sync_from_snmp")||"从SNMP同步"}</button>
            `);const h=e.querySelector("#sync-mac-empty-btn");h==null||h.addEventListener("click",()=>c());return}r=x.filter(h=>h.ip_address&&h.ip_address.includes(".")),s=x.filter(h=>h.ip_address&&h.ip_address.includes(":"));const T=e.querySelector('[data-tab="ipv4"]'),P=e.querySelector('[data-tab="ipv6"]');T&&(T.textContent=`IPv4 (${r.length})`),P&&(P.textContent=`IPv6 (${s.length})`);const _=e.querySelector("#ipv4-table-container"),$=e.querySelector("#ipv6-table-container");if(_&&(_.innerHTML=M(r,"ipv4")),$&&($.innerHTML=M(s,"ipv6")),_&&k(_),$&&k($),o){const h=e.querySelectorAll(".tab-btn"),A=e.querySelectorAll(".tab-content");h.forEach(f=>{f.addEventListener("click",()=>{h.forEach(E=>E.classList.remove("active")),f.classList.add("active"),A.forEach(E=>E.classList.remove("active"));const v=e.querySelector(`#${f.dataset.tab}-tab`);v&&v.classList.add("active")})});const L=e.querySelector("#ipv4-search"),S=e.querySelector("#ipv6-search");i=()=>{const f=C(r,(L==null?void 0:L.value)||""),v=e.querySelector("#ipv4-table-container");v&&(v.innerHTML=M(f,"ipv4"),k(v))},d=()=>{const f=C(s,(S==null?void 0:S.value)||""),v=e.querySelector("#ipv6-table-container");v&&(v.innerHTML=M(f,"ipv6"),k(v))},L==null||L.addEventListener("input",i),S==null||S.addEventListener("input",d),p==null||p.addEventListener("click",c),o=!1}n&&(n.style.display="none"),b&&(b.style.display="block")}else n&&(n.innerHTML=`<p style="color: red;">${t("switch.load_mac_failed")||"加载MAC表失败"}: ${q.message||t("common.unknown_error")||"未知错误"}</p>`)}catch(m){const w=m;n&&(n.innerHTML=`<p style="color: red;">${t("common.load_failed")||"加载失败"}: ${w.message}</p>`)}finally{p&&(p.disabled=!1)}},c=async()=>{const n=e.querySelector("#arp-loading"),b=e.querySelector("#arp-content"),y=e.querySelector("#sync-mac-btn");n&&(n.style.display="block"),b&&(b.style.display="none"),y&&(y.disabled=!0),n&&(n.innerHTML=`
        <div class="spinner"></div>
        <p style="margin-top: 10px; color: #666;">${t("switch.syncing_mac")||"正在从SNMP同步MAC表..."}</p>
      `);try{const p=await H(`/api/switches/${a}/macs/sync`,{});p.success?await l():n&&(n.innerHTML=`<p style="color: red;">${t("switch.sync_failed")||"同步失败"}: ${p.message||t("common.unknown_error")||"未知错误"}</p>`)}catch(p){const m=p;n&&(n.innerHTML=`<p style="color: red;">${t("switch.sync_failed")||"同步失败"}: ${m.message}</p>`)}finally{y&&(y.disabled=!1)}};await l()}function M(a,e){if(a.length===0)return`<p style="text-align: center; color: #666; padding: 20px;">${t("switch.no_ip_data")||"暂无"}${e==="ipv4"?"IPv4":"IPv6"}${t("switch.data")||"数据"}</p>`;const o=I(a,e);let i="",d=0;for(const[r,s]of Object.entries(o)){const l=`${e}-group-${d}`;i+=`<div style="margin-bottom: 10px;">
      <div class="network-group-header" data-target="${l}" style="background: #f5f5f5; padding: 8px 12px; font-weight: bold; border-left: 3px solid #4CAF50; cursor: pointer; display: flex; justify-content: space-between; align-items: center; user-select: none;">
        <span>${u(r)} (${s.length}${t("switch.entries")||"条"})</span>
        <span class="collapse-icon" style="transition: transform 0.2s; transform: rotate(-90deg);">▼</span>
      </div>
      <div id="${l}" class="network-group-content" style="display: none;">
        <table style="width:100%; border-collapse: collapse;">
          <tr><th style="border:1px solid #ddd; padding:6px; text-align:left; background:#fafafa;">${t("switch.ip_address")||"IP地址"}</th><th style="border:1px solid #ddd; padding:6px; text-align:left; background:#fafafa;">${t("switch.mac_address")||"MAC地址"}</th></tr>`,s.forEach(c=>{i+=`<tr><td style="border:1px solid #ddd; padding:6px;">${u(c.ip_address)}</td><td style="border:1px solid #ddd; padding:6px;">${u(c.mac_address)}</td></tr>`}),i+="</table></div></div>",d++}return i}function k(a){a.querySelectorAll(".network-group-header").forEach(e=>{e.addEventListener("click",()=>{const o=e.dataset.target,i=o?a.querySelector(`#${o}`):null,d=e.querySelector(".collapse-icon");i&&i.style.display==="none"?(i.style.display="block",d&&(d.style.transform="rotate(0deg)")):i&&(i.style.display="none",d&&(d.style.transform="rotate(-90deg)"))})})}function I(a,e){const o={};a.forEach(d=>{let r;if(e==="ipv4"){const s=d.ip_address.split(".");r=s.length>=3?`${s[0]}.${s[1]}.${s[2]}.0/24`:t("switch.unknown_network")||"未知网段"}else{const s=d.ip_address.split(":");s.length>=2?d.ip_address.startsWith("fe80")?r="fe80::/10 (Link-Local)":d.ip_address.startsWith("fd")||d.ip_address.startsWith("fc")?r=`${s[0]}::/16 (ULA)`:s[0]==="240e"||s[0]==="2409"||s[0]==="2408"?r=`${s.slice(0,4).join(":")}::/64 (Public)`:r=`${s.slice(0,4).join(":")}::/64`:r=t("switch.unknown_network")||"未知网段"}o[r]||(o[r]=[]),o[r].push(d)});const i={};return Object.keys(o).sort().forEach(d=>{i[d]=o[d].sort((r,s)=>{if(e==="ipv4"){const l=r.ip_address.split(".").map(Number),c=s.ip_address.split(".").map(Number);for(let n=0;n<4;n++)if(l[n]!==c[n])return l[n]-c[n];return 0}return r.ip_address.localeCompare(s.ip_address)})}),i}function C(a,e){if(!e)return a;const o=e.toLowerCase();return a.filter(i=>i.ip_address.toLowerCase().includes(o)||i.mac_address.toLowerCase().includes(o))}async function z(a){const e=document.createElement("div");e.className="modal active",e.id="lldp-modal-"+Date.now(),e.innerHTML=`
    <div class="modal-content" style="max-width: 900px;">
      <div class="modal-header">
        <h3>${t("switch.lldp_neighbors")||"LLDP邻居信息"} <span id="lldp-switch-name"></span></h3>
        <span class="close lldp-modal-close">&times;</span>
      </div>
      <div class="modal-body" style="max-height: 500px; overflow-y: auto;">
        <div id="lldp-loading" style="text-align: center; padding: 40px;">
          <div class="spinner"></div>
          <p style="margin-top: 10px; color: #666;">${t("switch.loading_lldp")||"正在加载LLDP邻居信息..."}</p>
        </div>
        <div id="lldp-content" style="display: none;"></div>
      </div>
      <div class="modal-footer">
        <button class="btn btn-primary" id="sync-lldp-btn">${t("switch.sync_from_snmp")||"从SNMP同步"}</button>
        <button class="btn btn-secondary lldp-modal-close">${t("common.close")||"关闭"}</button>
      </div>
    </div>
  `,document.body.appendChild(e),e.querySelectorAll(".lldp-modal-close").forEach(r=>{r.addEventListener("click",()=>e.remove())}),e.addEventListener("click",r=>{r.target===e&&e.remove()});const o=async()=>{var r;const s=e.querySelector("#lldp-loading"),l=e.querySelector("#lldp-content"),c=e.querySelector("#lldp-switch-name"),n=e.querySelector("#sync-lldp-btn");s&&(s.style.display="block"),l&&(l.style.display="none"),n&&(n.disabled=!0);try{const b=await g(`/api/switches/${a}`);if(!b.success){s&&(s.innerHTML=`<p style="color: red;">${t("switch.fetch_info_failed")||"获取交换机信息失败"}</p>`);return}const y=b.data;c&&(c.textContent=`- ${y.name}`);const p=await g(`/api/switches/${a}/lldp-neighbors`);if(p.success){const m=p.data||[];if(c&&(c.textContent=`- ${y.name} (${m.length}${t("switch.entries")||"条"})`),m.length===0){s&&(s.innerHTML=`
              <p style="color: #666; margin-bottom: 10px;">${t("switch.no_lldp_data")||'暂无LLDP数据，请点击"从SNMP同步"按钮获取'}</p>
              <button class="btn btn-primary" id="sync-lldp-empty-btn">${t("switch.sync_from_snmp")||"从SNMP同步"}</button>
            `),(r=e.querySelector("#sync-lldp-empty-btn"))==null||r.addEventListener("click",()=>i());return}l&&(l.innerHTML=j(m)),s&&(s.style.display="none"),l&&(l.style.display="block")}else s&&(s.innerHTML=`<p style="color: red;">${t("switch.load_lldp_failed")||"加载LLDP邻居失败"}: ${p.message||t("common.unknown_error")||"未知错误"}</p>`)}catch(b){const y=b;s&&(s.innerHTML=`<p style="color: red;">${t("common.load_failed")||"加载失败"}: ${y.message}</p>`)}finally{n&&(n.disabled=!1)}},i=async()=>{const r=e.querySelector("#lldp-loading"),s=e.querySelector("#lldp-content"),l=e.querySelector("#sync-lldp-btn");r&&(r.style.display="block"),s&&(s.style.display="none"),l&&(l.disabled=!0),r&&(r.innerHTML=`
        <div class="spinner"></div>
        <p style="margin-top: 10px; color: #666;">${t("switch.syncing_lldp")||"正在从SNMP同步LLDP信息..."}</p>
      `);try{const c=await H(`/api/switches/${a}/lldp/sync`,{});c.success?await o():r&&(r.innerHTML=`<p style="color: red;">${t("switch.sync_failed")||"同步失败"}: ${c.message||t("common.unknown_error")||"未知错误"}</p>`)}catch(c){const n=c;r&&(r.innerHTML=`<p style="color: red;">${t("switch.sync_failed")||"同步失败"}: ${n.message}</p>`)}finally{l&&(l.disabled=!1)}},d=e.querySelector("#sync-lldp-btn");d==null||d.addEventListener("click",i),await o()}function j(a){const e=o=>/^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$/.test(o);return`
    <table class="table table-bordered" style="border-collapse: collapse; width: 100%; margin-top: 10px;">
      <thead>
        <tr style="background-color: var(--bg-secondary, #f0f2f5);">
          <th style="width: 60px; text-align: center; border: 1px solid var(--border-color, #ddd); padding: 12px 8px; font-weight: 600;">#</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${t("switch.local_port")||"本地端口"}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${t("switch.neighbor_device")||"邻居设备"}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${t("switch.neighbor_port")||"邻居端口"}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">Chassis ID</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${t("switch.system_desc")||"系统描述"}</th>
        </tr>
      </thead>
      <tbody>
        ${a.map((o,i)=>{let d="-";o.neighbor_port_id&&!e(o.neighbor_port_id)?d=o.neighbor_port_id:o.neighbor_port_desc?d=o.neighbor_port_desc:o.neighbor_port_id&&(d=o.neighbor_port_id);const r=o.neighbor_sys_name||o.remote_system_name||"-",s=o.neighbor_chassis_id||o.remote_chassis_id||"-",l=o.neighbor_sys_desc||o.remote_system_description||"-";return`
          <tr style="background-color: ${i%2===0?"var(--bg-primary, #fff)":"var(--bg-tertiary, #fafbfc)"};">
            <td style="text-align: center; color: var(--text-muted, #888); border: 1px solid var(--border-color, #ddd); padding: 10px 8px;">${i+1}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px; font-weight: 500;">${u(o.local_port||"-")}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px;">${u(r)}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px;">${u(d)}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px; font-family: monospace; font-size: 0.9em;">${u(s)}</td>
            <td style="max-width: 250px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; border: 1px solid var(--border-color, #ddd); padding: 10px; font-size: 0.9em; color: var(--text-secondary, #666);" title="${u(l)}">${u(l)}</td>
          </tr>
        `}).join("")}
      </tbody>
    </table>
  `}async function W(){try{const a=await g("/api/switches?page_size=1000");if(a.success){const e=a.data,o=(e==null?void 0:e.items)||(Array.isArray(e)?e:[]),i=N.get("lldp-switch-select");i&&(i.innerHTML=`<option value="">${t("switch.select_switch")||"选择交换机..."}</option>`+o.map(d=>`<option value="${d.id}">${d.name}</option>`).join(""))}}catch(a){console.error("加载交换机列表失败:",a)}}export{k as bindCollapseEvents,C as filterEntries,I as groupByNetwork,W as loadSwitchesForLldp,j as renderLldpTable,M as renderMacTable,F as viewArpTable,z as viewLldpNeighbors};
