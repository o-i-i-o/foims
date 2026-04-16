import{t as s,b as _,e as N,f as v,d as A}from"./authManager-7S1aOnxE.js";async function F(a){const t=document.createElement("div");t.className="modal active",t.id="arp-modal-"+Date.now(),t.innerHTML=`
    <div class="modal-content" style="max-width: 800px;">
      <div class="modal-header">
        <h3>${s("switch.mac_table")||"MAC表"} <span id="arp-switch-name"></span></h3>
        <span class="close arp-modal-close">&times;</span>
      </div>
      <div class="modal-body" style="max-height: 500px; overflow-y: auto;">
        <div id="arp-loading" style="text-align: center; padding: 40px;">
          <div class="spinner"></div>
          <p style="margin-top: 10px; color: #666;">${s("switch.loading_mac")||"正在加载MAC表..."}</p>
        </div>
        <div id="arp-content" style="display: none;">
          <div class="tab-container">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;">
              <div class="tab-buttons" style="display: flex; gap: 5px;">
                <button class="tab-btn active" data-tab="ipv4">IPv4</button>
                <button class="tab-btn" data-tab="ipv6">IPv6</button>
              </div>
              <button class="btn btn-sm btn-primary" id="sync-mac-btn">
                <span>${s("switch.sync_from_snmp")||"从SNMP同步"}</span>
              </button>
            </div>
            <div class="tab-content active" id="ipv4-tab">
              <div style="margin-bottom: 10px;">
                <input type="text" id="ipv4-search" placeholder="${s("switch.search_ip_mac")||"搜索IP或MAC地址..."}" style="width: 100%; padding: 8px; border: 1px solid #ccc; border-radius: 4px;">
              </div>
              <div id="ipv4-table-container" style="max-height: 350px; overflow-y: auto;"></div>
            </div>
            <div class="tab-content" id="ipv6-tab">
              <div style="margin-bottom: 10px;">
                <input type="text" id="ipv6-search" placeholder="${s("switch.search_ip_mac")||"搜索IP或MAC地址..."}" style="width: 100%; padding: 8px; border: 1px solid #ccc; border-radius: 4px;">
              </div>
              <div id="ipv6-table-container" style="max-height: 350px; overflow-y: auto;"></div>
            </div>
          </div>
        </div>
      </div>
      <div class="modal-footer">
        <button class="btn btn-secondary arp-modal-close">${s("common.close")||"关闭"}</button>
      </div>
    </div>
  `,document.body.appendChild(t),t.querySelectorAll(".arp-modal-close").forEach(i=>{i.addEventListener("click",()=>t.remove())}),t.addEventListener("click",i=>{i.target===t&&t.remove()});let r=!0,n=null,d=null,e=[],l=[];const p=async()=>{const i=t.querySelector("#arp-loading"),b=t.querySelector("#arp-content"),f=t.querySelector("#arp-switch-name"),y=t.querySelector("#sync-mac-btn");i&&(i.style.display="block"),b&&(b.style.display="none"),y&&(y.disabled=!0);try{const h=await _(`/api/switches/${a}`);if(!h.success){i&&(i.innerHTML=`<p style="color: red;">${s("switch.fetch_info_failed")||"获取交换机信息失败"}</p>`);return}const $=h.data;f&&(f.textContent=`- ${$.name}`);const q=await _(`/api/switches/${a}/macs`);if(q.success){const L=q.data||[];if(f&&(f.textContent=`- ${$.name} (${L.length}${s("switch.entries")||"条"})`),L.length===0){i&&(i.innerHTML=`
              <p style="color: #666; margin-bottom: 10px;">${s("switch.no_mac_data")||'暂无MAC数据，请点击"从SNMP同步"按钮获取'}</p>
              <button class="btn btn-primary" id="sync-mac-empty-btn">${s("switch.sync_from_snmp")||"从SNMP同步"}</button>
            `);const m=t.querySelector("#sync-mac-empty-btn");m==null||m.addEventListener("click",()=>c());return}e=L.filter(m=>m.ip_address&&m.ip_address.includes(".")),l=L.filter(m=>m.ip_address&&m.ip_address.includes(":"));const P=t.querySelector('[data-tab="ipv4"]'),T=t.querySelector('[data-tab="ipv6"]');P&&(P.textContent=`IPv4 (${e.length})`),T&&(T.textContent=`IPv6 (${l.length})`);const M=t.querySelector("#ipv4-table-container"),S=t.querySelector("#ipv6-table-container");if(M&&(M.innerHTML=k(e,"ipv4")),S&&(S.innerHTML=k(l,"ipv6")),M&&E(M),S&&E(S),r){const m=t.querySelectorAll(".tab-btn"),D=t.querySelectorAll(".tab-content");m.forEach(g=>{g.addEventListener("click",()=>{m.forEach(C=>C.classList.remove("active")),g.classList.add("active"),D.forEach(C=>C.classList.remove("active"));const u=t.querySelector(`#${g.dataset.tab}-tab`);u&&u.classList.add("active")})});const w=t.querySelector("#ipv4-search"),x=t.querySelector("#ipv6-search");n=()=>{const g=H(e,(w==null?void 0:w.value)||""),u=t.querySelector("#ipv4-table-container");u&&(u.innerHTML=k(g,"ipv4"),E(u))},d=()=>{const g=H(l,(x==null?void 0:x.value)||""),u=t.querySelector("#ipv6-table-container");u&&(u.innerHTML=k(g,"ipv6"),E(u))},w==null||w.addEventListener("input",n),x==null||x.addEventListener("input",d),y==null||y.addEventListener("click",c),r=!1}i&&(i.style.display="none"),b&&(b.style.display="block")}else i&&(i.innerHTML=`<p style="color: red;">${s("switch.load_mac_failed")||"加载MAC表失败"}: ${q.message||s("common.unknown_error")||"未知错误"}</p>`)}catch(h){const $=h;i&&(i.innerHTML=`<p style="color: red;">${s("common.load_failed")||"加载失败"}: ${$.message}</p>`)}finally{y&&(y.disabled=!1)}},c=async()=>{const i=t.querySelector("#arp-loading"),b=t.querySelector("#arp-content"),f=t.querySelector("#sync-mac-btn");i&&(i.style.display="block"),b&&(b.style.display="none"),f&&(f.disabled=!0),i&&(i.innerHTML=`
        <div class="spinner"></div>
        <p style="margin-top: 10px; color: #666;">${s("switch.syncing_mac")||"正在从SNMP同步MAC表..."}</p>
      `);try{const y=await A(`/api/switches/${a}/macs/sync`,{});y.success?await p():i&&(i.innerHTML=`<p style="color: red;">${s("switch.sync_failed")||"同步失败"}: ${y.message||s("common.unknown_error")||"未知错误"}</p>`)}catch(y){const h=y;i&&(i.innerHTML=`<p style="color: red;">${s("switch.sync_failed")||"同步失败"}: ${h.message}</p>`)}finally{f&&(f.disabled=!1)}};await p()}function k(a,t){if(a.length===0)return`<p style="text-align: center; color: #666; padding: 20px;">${s("switch.no_ip_data")||"暂无"}${t==="ipv4"?"IPv4":"IPv6"}${s("switch.data")||"数据"}</p>`;const o=I(a,t);let r="",n=0;for(const[d,e]of Object.entries(o)){const l=`${t}-group-${n}`;r+=`<div style="margin-bottom: 10px;">
      <div class="network-group-header" data-target="${l}" style="background: #f5f5f5; padding: 8px 12px; font-weight: bold; border-left: 3px solid #4CAF50; cursor: pointer; display: flex; justify-content: space-between; align-items: center; user-select: none;">
        <span>${v(d)} (${e.length}${s("switch.entries")||"条"})</span>
        <span class="collapse-icon" style="transition: transform 0.2s; transform: rotate(-90deg);">▼</span>
      </div>
      <div id="${l}" class="network-group-content" style="display: none;">
        <table style="width:100%; border-collapse: collapse;">
          <tr><th style="border:1px solid #ddd; padding:6px; text-align:left; background:#fafafa;">${s("switch.ip_address")||"IP地址"}</th><th style="border:1px solid #ddd; padding:6px; text-align:left; background:#fafafa;">${s("switch.mac_address")||"MAC地址"}</th></tr>`,e.forEach(i=>{r+=`<tr><td style="border:1px solid #ddd; padding:6px;">${v(i.ip_address)}</td><td style="border:1px solid #ddd; padding:6px;">${v(i.mac_address)}</td></tr>`}),r+="</table></div></div>",n++}return r}function E(a){a.querySelectorAll(".network-group-header").forEach(t=>{t.addEventListener("click",()=>{const o=t.dataset.target,r=o?a.querySelector(`#${o}`):null,n=t.querySelector(".collapse-icon");r&&r.style.display==="none"?(r.style.display="block",n&&(n.style.transform="rotate(0deg)")):r&&(r.style.display="none",n&&(n.style.transform="rotate(-90deg)"))})})}function I(a,t){const o={};a.forEach(n=>{let d;if(t==="ipv4"){const e=n.ip_address.split(".");d=e.length>=3?`${e[0]}.${e[1]}.${e[2]}.0/24`:s("switch.unknown_network")||"未知网段"}else{const e=n.ip_address.split(":");e.length>=2?n.ip_address.startsWith("fe80")?d="fe80::/10 (Link-Local)":n.ip_address.startsWith("fd")||n.ip_address.startsWith("fc")?d=`${e[0]}::/16 (ULA)`:e[0]==="240e"||e[0]==="2409"||e[0]==="2408"?d=`${e.slice(0,4).join(":")}::/64 (Public)`:d=`${e.slice(0,4).join(":")}::/64`:d=s("switch.unknown_network")||"未知网段"}o[d]||(o[d]=[]),o[d].push(n)});const r={};return Object.keys(o).sort().forEach(n=>{r[n]=o[n].sort((d,e)=>{if(t==="ipv4"){const l=d.ip_address.split(".").map(Number),p=e.ip_address.split(".").map(Number);for(let c=0;c<4;c++)if(l[c]!==p[c])return l[c]-p[c];return 0}return d.ip_address.localeCompare(e.ip_address)})}),r}function H(a,t){if(!t)return a;const o=t.toLowerCase();return a.filter(r=>r.ip_address.toLowerCase().includes(o)||r.mac_address.toLowerCase().includes(o))}async function R(a){const t=document.createElement("div");t.className="modal active",t.id="lldp-modal-"+Date.now(),t.innerHTML=`
    <div class="modal-content" style="max-width: 900px;">
      <div class="modal-header">
        <h3>${s("switch.lldp_neighbors")||"LLDP邻居信息"} <span id="lldp-switch-name"></span></h3>
        <span class="close lldp-modal-close">&times;</span>
      </div>
      <div class="modal-body" style="max-height: 500px; overflow-y: auto;">
        <div id="lldp-loading" style="text-align: center; padding: 40px;">
          <div class="spinner"></div>
          <p style="margin-top: 10px; color: #666;">${s("switch.loading_lldp")||"正在加载LLDP邻居信息..."}</p>
        </div>
        <div id="lldp-content" style="display: none;"></div>
      </div>
      <div class="modal-footer">
        <button class="btn btn-primary" id="sync-lldp-btn">${s("switch.sync_from_snmp")||"从SNMP同步"}</button>
        <button class="btn btn-secondary lldp-modal-close">${s("common.close")||"关闭"}</button>
      </div>
    </div>
  `,document.body.appendChild(t),t.querySelectorAll(".lldp-modal-close").forEach(e=>{e.addEventListener("click",()=>t.remove())}),t.addEventListener("click",e=>{e.target===t&&t.remove()});const r=async()=>{var i;const e=t.querySelector("#lldp-loading"),l=t.querySelector("#lldp-content"),p=t.querySelector("#lldp-switch-name"),c=t.querySelector("#sync-lldp-btn");e&&(e.style.display="block"),l&&(l.style.display="none"),c&&(c.disabled=!0);try{const b=await _(`/api/switches/${a}`);if(!b.success){e&&(e.innerHTML=`<p style="color: red;">${s("switch.fetch_info_failed")||"获取交换机信息失败"}</p>`);return}const f=b.data;p&&(p.textContent=`- ${f.name}`);const y=await _(`/api/switches/${a}/lldp-neighbors`);if(y.success){const h=y.data||[];if(p&&(p.textContent=`- ${f.name} (${h.length}${s("switch.entries")||"条"})`),h.length===0){e&&(e.innerHTML=`
              <p style="color: #666; margin-bottom: 10px;">${s("switch.no_lldp_data")||'暂无LLDP数据，请点击"从SNMP同步"按钮获取'}</p>
              <button class="btn btn-primary" id="sync-lldp-empty-btn">${s("switch.sync_from_snmp")||"从SNMP同步"}</button>
            `),(i=t.querySelector("#sync-lldp-empty-btn"))==null||i.addEventListener("click",()=>n());return}l&&(l.innerHTML=j(h)),e&&(e.style.display="none"),l&&(l.style.display="block")}else e&&(e.innerHTML=`<p style="color: red;">${s("switch.load_lldp_failed")||"加载LLDP邻居失败"}: ${y.message||s("common.unknown_error")||"未知错误"}</p>`)}catch(b){const f=b;e&&(e.innerHTML=`<p style="color: red;">${s("common.load_failed")||"加载失败"}: ${f.message}</p>`)}finally{c&&(c.disabled=!1)}},n=async()=>{const e=t.querySelector("#lldp-loading"),l=t.querySelector("#lldp-content"),p=t.querySelector("#sync-lldp-btn");e&&(e.style.display="block"),l&&(l.style.display="none"),p&&(p.disabled=!0),e&&(e.innerHTML=`
        <div class="spinner"></div>
        <p style="margin-top: 10px; color: #666;">${s("switch.syncing_lldp")||"正在从SNMP同步LLDP信息..."}</p>
      `);try{const c=await A(`/api/switches/${a}/lldp/sync`,{});c.success?await r():e&&(e.innerHTML=`<p style="color: red;">${s("switch.sync_failed")||"同步失败"}: ${c.message||s("common.unknown_error")||"未知错误"}</p>`)}catch(c){const i=c;e&&(e.innerHTML=`<p style="color: red;">${s("switch.sync_failed")||"同步失败"}: ${i.message}</p>`)}finally{p&&(p.disabled=!1)}},d=t.querySelector("#sync-lldp-btn");d==null||d.addEventListener("click",n),await r()}function j(a){const t=o=>/^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$/.test(o);return`
    <table class="table table-bordered" style="border-collapse: collapse; width: 100%; margin-top: 10px;">
      <thead>
        <tr style="background-color: var(--bg-secondary, #f0f2f5);">
          <th style="width: 60px; text-align: center; border: 1px solid var(--border-color, #ddd); padding: 12px 8px; font-weight: 600;">#</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${s("switch.local_port")||"本地端口"}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${s("switch.neighbor_device")||"邻居设备"}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${s("switch.neighbor_port")||"邻居端口"}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">Chassis ID</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${s("switch.system_desc")||"系统描述"}</th>
        </tr>
      </thead>
      <tbody>
        ${a.map((o,r)=>{let n="-";o.neighbor_port_id&&!t(o.neighbor_port_id)?n=o.neighbor_port_id:o.neighbor_port_desc?n=o.neighbor_port_desc:o.neighbor_port_id&&(n=o.neighbor_port_id);const d=o.neighbor_sys_name||o.remote_system_name||"-",e=o.neighbor_chassis_id||o.remote_chassis_id||"-",l=o.neighbor_sys_desc||o.remote_system_description||"-";return`
          <tr style="background-color: ${r%2===0?"var(--bg-primary, #fff)":"var(--bg-tertiary, #fafbfc)"};">
            <td style="text-align: center; color: var(--text-muted, #888); border: 1px solid var(--border-color, #ddd); padding: 10px 8px;">${r+1}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px; font-weight: 500;">${v(o.local_port||"-")}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px;">${v(d)}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px;">${v(n)}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px; font-family: monospace; font-size: 0.9em;">${v(e)}</td>
            <td style="max-width: 250px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; border: 1px solid var(--border-color, #ddd); padding: 10px; font-size: 0.9em; color: var(--text-secondary, #666);" title="${v(l)}">${v(l)}</td>
          </tr>
        `}).join("")}
      </tbody>
    </table>
  `}async function z(){try{const a=await _("/api/switches?page_size=1000");if(a.success){const t=a.data,o=(t==null?void 0:t.items)||(Array.isArray(t)?t:[]),r=N.get("lldp-switch-select");r&&(r.innerHTML=`<option value="">${s("switch.select_switch")||"选择交换机..."}</option>`+o.map(n=>`<option value="${n.id}">${n.name}</option>`).join(""))}}catch(a){console.error("加载交换机列表失败:",a)}}export{E as bindCollapseEvents,H as filterEntries,I as groupByNetwork,z as loadSwitchesForLldp,j as renderLldpTable,k as renderMacTable,F as viewArpTable,R as viewLldpNeighbors};
