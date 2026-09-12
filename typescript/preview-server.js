const http = require('http');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const UI_PATH = path.join(__dirname, 'n8n-rust/crates/n8n-server/ui/app.html');
const PORT = process.env.PORT ? parseInt(process.env.PORT) : 3000;
const HOST = '0.0.0.0';

let hooks = new Map();
let runs = [];
let nextId = 1;

// persistence in-memory with file fallback for demo
let workflows = []; // {id, name, nodes, connections, active, settings, tags, version_id, created_at, updated_at}
let credentials = []; // {id, name, type, data (encrypted mock), created_at, updated_at}
let executions = []; // {id, workflow_id, workflow_name, status, started_at, finished_at, data}
let credentialTypes = [
  {name:'httpHeaderAuth', displayName:'Header Auth', properties:[{displayName:'Name', name:'name', type:'string'}, {displayName:'Value', name:'value', type:'string', typeOptions:{password:true}}]},
  {name:'httpBasicAuth', displayName:'Basic Auth', properties:[{displayName:'User', name:'user', type:'string'}, {displayName:'Password', name:'password', type:'string', typeOptions:{password:true}}]},
  {name:'oAuth2Api', displayName:'OAuth2 API', properties:[{displayName:'Access Token', name:'accessToken', type:'string', typeOptions:{password:true}}]},
  {name:'slackApi', displayName:'Slack API', properties:[{displayName:'Access Token', name:'accessToken', type:'string', typeOptions:{password:true}}]},
  {name:'postgres', displayName:'Postgres', properties:[{displayName:'Host', name:'host', type:'string'}, {displayName:'Database', name:'database', type:'string'}, {displayName:'User', name:'user', type:'string'}, {displayName:'Password', name:'password', type:'string', typeOptions:{password:true}}]},
  {name:'mySql', displayName:'MySQL', properties:[{displayName:'Host', name:'host', type:'string'}, {displayName:'Password', name:'password', type:'string', typeOptions:{password:true}}]},
  {name:'mongoDb', displayName:'MongoDB', properties:[{displayName:'Connection String', name:'connectionString', type:'string', typeOptions:{password:true}}]},
  {name:'redis', displayName:'Redis', properties:[{displayName:'Host', name:'host', type:'string'}, {displayName:'Password', name:'password', type:'string', typeOptions:{password:true}}]},
  {name:'smtp', displayName:'SMTP', properties:[{displayName:'User', name:'user', type:'string'}, {displayName:'Password', name:'password', type:'string', typeOptions:{password:true}}]},
  {name:'githubApi', displayName:'GitHub API', properties:[{displayName:'Access Token', name:'accessToken', type:'string', typeOptions:{password:true}}]},
  {name:'googleApi', displayName:'Google API', properties:[{displayName:'Client ID', name:'clientId', type:'string'}, {displayName:'Client Secret', name:'clientSecret', type:'string', typeOptions:{password:true}}]},
  {name:'aws', displayName:'AWS', properties:[{displayName:'Access Key', name:'accessKeyId', type:'string'}, {displayName:'Secret Key', name:'secretAccessKey', type:'string', typeOptions:{password:true}}]},
];

// simple AES-GCM mock encrypt (like Rust impl)
const ENC_KEY = crypto.randomBytes(32); // 32 bytes = 256 bit
function encrypt_data(obj){
  const iv = crypto.randomBytes(12);
  const cipher = crypto.createCipheriv('aes-256-gcm', ENC_KEY, iv);
  const plaintext = JSON.stringify(obj);
  let enc = cipher.update(plaintext, 'utf8', 'base64');
  enc += cipher.final('base64');
  const tag = cipher.getAuthTag();
  return { iv: iv.toString('base64'), data: enc, tag: tag.toString('base64') };
}
function decrypt_data(encObj){
  try{
    const iv = Buffer.from(encObj.iv, 'base64');
    const tag = Buffer.from(encObj.tag, 'base64');
    const decipher = crypto.createDecipheriv('aes-256-gcm', ENC_KEY, iv);
    decipher.setAuthTag(tag);
    let dec = decipher.update(encObj.data, 'base64', 'utf8');
    dec += decipher.final('utf8');
    return JSON.parse(dec);
  }catch{ return {}; }
}
function masked(cred){
  // return masked view: type + name + id + data with *** 
  const raw = cred._enc ? decrypt_data(cred._enc) : cred.data||{};
  const maskedData = {};
  for(const k of Object.keys(raw)){
    const v = String(raw[k]||'');
    maskedData[k] = v.length>0 ? '•'.repeat(Math.min(8, v.length)) : '';
  }
  return { id: cred.id, name: cred.name, type: cred.type, data: maskedData, created_at: cred.created_at, updated_at: cred.updated_at };
}

function json(res, code, data){
  res.writeHead(code, {
    'Content-Type':'application/json',
    'Access-Control-Allow-Origin':'*',
    'Access-Control-Allow-Methods':'GET,POST,PUT,PATCH,DELETE,OPTIONS',
    'Access-Control-Allow-Headers':'Content-Type,Authorization,X-Requested-With',
  });
  res.end(JSON.stringify(data));
}
function text(res, code, data, ctype='text/html'){
  res.writeHead(code, {
    'Content-Type': ctype,
    'Access-Control-Allow-Origin':'*',
  });
  res.end(data);
}

function mockTopo(wf){
  const enabled = (wf.nodes||[]).filter(n=>!n.disabled);
  const incoming = new Map(enabled.map(n=>[n.name,[]]));
  for(const n of enabled){
    const brs = (wf.connections?.[n.name]?.main)||[];
    for(const b of brs){
      if(!Array.isArray(b)) continue;
      for(const l of b){
        if(l && l.node && incoming.has(l.node)){
          incoming.get(l.node).push(n.name);
        }
      }
    }
  }
  const done=new Set(), order=[];
  let progress=true;
  while(progress && order.length<enabled.length){
    progress=false;
    for(const n of enabled){
      if(done.has(n.name)) continue;
      const deps=incoming.get(n.name)||[];
      if(deps.every(d=>done.has(d))){
        done.add(n.name); order.push(n.name); progress=true;
      }
    }
  }
  if(order.length!==enabled.length){
    const stuck=enabled.filter(n=>!done.has(n.name)).map(n=>n.name).join(", ");
    throw new Error("cycle detected, stuck at: "+stuck);
  }
  return order;
}

function mockValidate(wf){
  const diags=[];
  const NODE_TYPES = new Set([
    "n8n-nodes-base.manualTrigger","n8n-nodes-base.scheduleTrigger","n8n-nodes-base.webhook",
    "n8n-nodes-base.set","n8n-nodes-base.if","n8n-nodes-base.filter","n8n-nodes-base.switch",
    "n8n-nodes-base.merge","n8n-nodes-base.sort","n8n-nodes-base.limit","n8n-nodes-base.httpRequest",
    "n8n-nodes-base.code","n8n-nodes-base.function","n8n-nodes-base.dateTime","n8n-nodes-base.noOp",
    "n8n-nodes-base.wait","n8n-nodes-base.respondToWebhook","n8n-nodes-base.stopAndError","n8n-nodes-base.executeCommand"
  ]);
  const names=new Set((wf.nodes||[]).map(n=>n.name));
  for(const n of (wf.nodes||[])){
    if(!NODE_TYPES.has(n.type)) diags.push({level:"error",message:`node '${n.name}': tipe tak dikenal '${n.type}'`,node:n.name});
  }
  for(const from of Object.keys(wf.connections||{})){
    const m=wf.connections[from]?.main; if(!Array.isArray(m)) continue;
    for(const br of m){ if(!Array.isArray(br)) continue; for(const l of br){ if(l&&l.node&&!names.has(l.node)) diags.push({level:"error",message:`edge gantung: '${from}' -> '${l.node}'`,node:from}); } }
  }
  try{ mockTopo(wf); }catch(e){ diags.push({level:"error",message:e.message}); }
  const targets=new Set();
  for(const n of (wf.nodes||[])){ for(const s of (wf.connections?.[n.name]?.main||[]).flat().filter(Boolean).map(x=>x.node).filter(Boolean)) targets.add(s); }
  for(const n of (wf.nodes||[])){
    const hasOut = (wf.connections?.[n.name]?.main||[]).flat().length>0;
    if(!hasOut && !targets.has(n.name) && (wf.nodes||[]).length>1) diags.push({level:"warning",message:`node '${n.name}' terisolasi`,node:n.name});
  }
  return diags;
}

function mockRun(wf){
  const order=mockTopo(wf);
  const outputs={}; const durations={};
  for(const name of order){
    const node=(wf.nodes||[]).find(n=>n.name===name);
    if(!node) continue;
    const type=node.type;
    let preds=[];
    for(const k of Object.keys(wf.connections||{})){
      const m=wf.connections[k]?.main||[];
      for(let bi=0; bi<m.length; bi++){
        const br=m[bi]; if(!Array.isArray(br)) continue;
        if(br.some(l=>l&&l.node===name)){
          const out=outputs[k];
          if(out && out[bi]) preds.push(...out[bi]);
        }
      }
    }
    let items=[];
    if(preds.length===0){
      if(type.includes("manualTrigger")||type.includes("scheduleTrigger")) items=[{}];
      else if(type.includes("webhook")) items=[{mode:"manual",timestamp:new Date().toISOString()}];
      else items=[{}];
    } else items=preds;

    if(type==="n8n-nodes-base.set"){
      const assign=node.parameters?.assignments?.assignments||[];
      const values=node.parameters?.values||{};
      items=items.map(it=>{
        const base={...it};
        for(const a of assign){ if(a.name) base[a.name]=a.value; }
        for(const [k,v] of Object.entries(values)){ base[k]=v; }
        return base;
      });
    } else if(type==="n8n-nodes-base.limit"){
      const max=node.parameters?.maxItems||1;
      items=items.slice(0,max);
    } else if(type==="n8n-nodes-base.filter"){
      items=items.filter((_,i)=>i%2===0);
    } else if(type==="n8n-nodes-base.code"||type==="n8n-nodes-base.function"){
      items=items.map(it=>({...it, coded:true, at:Date.now()}));
    } else if(type==="n8n-nodes-base.executeCommand"){
      items=[{command: node.parameters?.command||"echo halo", stdout:"halo\n", stderr:"", exitCode:0}];
    } else if(type==="n8n-nodes-base.dateTime"){
      items=items.map(it=>({...it, currentDate:new Date().toISOString()}));
    } else if(type==="n8n-nodes-base.wait"){
      // wait marker
      items=items.map(it=>({...it, __wait:true, waitId: 'wait-'+nextId}));
    }
    outputs[name]=[items];
    durations[name]=Math.floor(Math.random()*60)+8;
  }
  const total=Object.values(durations).reduce((a,b)=>a+b,0);
  return {outputs, order, durations_ms:durations, total_ms:total};
}

const server = http.createServer((req,res)=>{
  const url = new URL(req.url, `http://${req.headers.host}`);
  const pathname = url.pathname;

  if(req.method==='OPTIONS'){
    res.writeHead(204, {
      'Access-Control-Allow-Origin':'*',
      'Access-Control-Allow-Methods':'GET,POST,PUT,PATCH,DELETE,OPTIONS',
      'Access-Control-Allow-Headers':'Content-Type,Authorization,X-Requested-With',
    });
    return res.end();
  }

  if(pathname==='/' && req.method==='GET'){
    try{
      const html = fs.readFileSync(UI_PATH,'utf8');
      return text(res,200,html);
    }catch(e){
      return text(res,500,"UI not found: "+e.message,'text/plain');
    }
  }

  if(pathname==='/health' && req.method==='GET'){
    return json(res,200,{status:"ok", uptime_secs: Math.floor(process.uptime()), version:"0.9.0", nodes:19, hooks:hooks.size, runs:runs.length, workflows:workflows.length, credentials:credentials.length});
  }
  if(pathname==='/api/nodes' && req.method==='GET'){
    return json(res,200,[
      "n8n-nodes-base.manualTrigger","n8n-nodes-base.scheduleTrigger","n8n-nodes-base.webhook",
      "n8n-nodes-base.set","n8n-nodes-base.if","n8n-nodes-base.filter","n8n-nodes-base.switch",
      "n8n-nodes-base.merge","n8n-nodes-base.sort","n8n-nodes-base.limit","n8n-nodes-base.httpRequest",
      "n8n-nodes-base.code","n8n-nodes-base.function","n8n-nodes-base.dateTime","n8n-nodes-base.noOp",
      "n8n-nodes-base.wait","n8n-nodes-base.respondToWebhook","n8n-nodes-base.stopAndError","n8n-nodes-base.executeCommand"
    ]);
  }
  if(pathname==='/api/credential-types' && req.method==='GET'){
    return json(res,200,credentialTypes);
  }
  if(pathname==='/api/credentials' && req.method==='GET'){
    return json(res,200,credentials.map(masked));
  }
  if(pathname==='/api/metrics' && req.method==='GET'){
    return json(res,200,{uptime_secs:Math.floor(process.uptime()), total_runs:runs.length, hooks:hooks.size, nodes:19, version:"0.9.0", workflows:workflows.length, credentials:credentials.length});
  }
  if(pathname==='/api/openapi.json' && req.method==='GET'){
    return json(res,200,{
      openapi:"3.0.0",
      info:{title:"n8n-rust API",version:"0.9.0"},
      paths:{
        "/":{get:{summary:"UI"}},
        "/health":{get:{summary:"Health"}},
        "/api/nodes":{get:{summary:"List nodes"}},
        "/api/validate":{post:{summary:"Validate workflow"}},
        "/api/explain":{post:{summary:"Explain topo"}},
        "/api/run":{post:{summary:"Run workflow"}},
        "/api/runs":{get:{summary:"List runs"}},
        "/api/workflows":{get:{summary:"List workflows"}, post:{summary:"Create workflow"}},
        "/api/credentials":{get:{summary:"List creds"}, post:{summary:"Create cred"}},
        "/api/credential-types":{get:{summary:"List cred types"}},
        "/api/executions":{get:{summary:"List executions"}},
        "/api/wait/resume/{id}":{post:{summary:"Resume wait"}},
        "/api/hooks":{get:{summary:"List hooks"}, post:{summary:"Create hook"}},
        "/hook/{path}":{get:{summary:"Webhook"}, post:{summary:"Webhook"}, put:{summary:"Webhook"}, delete:{summary:"Webhook"}, patch:{summary:"Webhook"}},
        "/ws/logs":{get:{summary:"WS logs"}},
        "/ws/executions":{get:{summary:"WS executions"}},
        "/api/metrics":{get:{summary:"Metrics"}}
      }
    });
  }
  if(pathname==='/api/runs' && req.method==='GET'){
    const limit = Math.min(100, parseInt(url.searchParams.get('limit')||'50'));
    return json(res,200,runs.slice(-limit).reverse());
  }
  if(pathname==='/api/executions' && req.method==='GET'){
    const limit = Math.min(100, parseInt(url.searchParams.get('limit')||'50'));
    return json(res,200,executions.slice(-limit).reverse());
  }
  if(pathname==='/api/workflows' && req.method==='GET'){
    const limit = Math.min(200, parseInt(url.searchParams.get('limit')||'100'));
    return json(res,200,workflows.slice(-limit).reverse());
  }
  if(pathname==='/api/hooks' && req.method==='GET'){
    return json(res,200,Array.from(hooks.keys()).sort());
  }

  // id-based routes
  if(pathname.startsWith('/api/workflows/') && req.method==='GET'){
    const id = pathname.replace('/api/workflows/','').split('/')[0];
    if(id==='activate' || id==='deactivate'){} else {
      const wf = workflows.find(w=>w.id===id);
      if(!wf) return text(res,404,'workflow not found','text/plain');
      return json(res,200,wf);
    }
  }
  if(pathname.startsWith('/api/credentials/') && req.method==='GET'){
    const id = pathname.replace('/api/credentials/','').split('/')[0];
    const cred = credentials.find(c=>c.id===id);
    if(!cred) return text(res,404,'credential not found','text/plain');
    return json(res,200,masked(cred));
  }
  if(pathname.startsWith('/api/hooks/') && req.method==='DELETE'){
    const p=pathname.replace('/api/hooks/','');
    const ok=hooks.delete(p);
    return json(res,200,ok);
  }
  if(pathname.startsWith('/api/workflows/') && req.method==='DELETE'){
    const id = pathname.replace('/api/workflows/','').split('/')[0];
    const idx = workflows.findIndex(w=>w.id===id);
    if(idx===-1) return text(res,404,'workflow not found','text/plain');
    workflows.splice(idx,1);
    return json(res,200,{deleted:true, id});
  }
  if(pathname.startsWith('/api/credentials/') && req.method==='DELETE'){
    const id = pathname.replace('/api/credentials/','').split('/')[0];
    const idx = credentials.findIndex(c=>c.id===id);
    if(idx===-1) return text(res,404,'credential not found','text/plain');
    credentials.splice(idx,1);
    return json(res,200,{deleted:true, id});
  }

  // POST/PUT need body
  if(['POST','PUT','PATCH'].includes(req.method)){
    let body='';
    req.on('data',chunk=>body+=chunk);
    req.on('end',()=>{
      let data;
      try{ data=JSON.parse(body||'{}'); }catch{ data={}; }

      // workflows CRUD
      if(pathname==='/api/workflows' && req.method==='POST'){
        const id = 'wf-'+Date.now()+'-'+Math.random().toString(36).slice(2,7);
        const now = new Date().toISOString();
        const wf = {
          id,
          name: data.name||'My workflow',
          nodes: data.nodes||[],
          connections: data.connections||{},
          active: !!data.active,
          settings: data.settings||{executionOrder:'v1'},
          tags: data.tags||[],
          version_id: 'v-'+Date.now(),
          created_at: now,
          updated_at: now,
        };
        workflows.push(wf);
        return json(res,201,wf);
      }
      if(pathname.startsWith('/api/workflows/') && req.method==='PUT'){
        const id = pathname.replace('/api/workflows/','').split('/')[0];
        const idx = workflows.findIndex(w=>w.id===id);
        if(idx===-1) return text(res,404,'workflow not found','text/plain');
        const now = new Date().toISOString();
        workflows[idx] = {...workflows[idx], ...data, id, updated_at: now, version_id:'v-'+Date.now()};
        return json(res,200,workflows[idx]);
      }
      if(pathname.match(/^\/api\/workflows\/[^\/]+\/activate$/) && req.method==='POST'){
        const id = pathname.split('/')[3];
        const wf = workflows.find(w=>w.id===id);
        if(!wf) return text(res,404,'not found','text/plain');
        wf.active=true; wf.updated_at=new Date().toISOString();
        return json(res,200,wf);
      }
      if(pathname.match(/^\/api\/workflows\/[^\/]+\/deactivate$/) && req.method==='POST'){
        const id = pathname.split('/')[3];
        const wf = workflows.find(w=>w.id===id);
        if(!wf) return text(res,404,'not found','text/plain');
        wf.active=false; wf.updated_at=new Date().toISOString();
        return json(res,200,wf);
      }

      // credentials CRUD
      if(pathname==='/api/credentials' && req.method==='POST'){
        const id = 'cred-'+Date.now()+'-'+Math.random().toString(36).slice(2,6);
        const now = new Date().toISOString();
        const enc = encrypt_data(data.data||{});
        const cred = { id, name: data.name||'My credential', type: data.type||'httpHeaderAuth', _enc: enc, data: data.data||{}, created_at: now, updated_at: now };
        credentials.push(cred);
        return json(res,201,masked(cred));
      }
      if(pathname.startsWith('/api/credentials/') && req.method==='PUT'){
        const id = pathname.replace('/api/credentials/','').split('/')[0];
        const idx = credentials.findIndex(c=>c.id===id);
        if(idx===-1) return text(res,404,'credential not found','text/plain');
        const now = new Date().toISOString();
        if(data.data) credentials[idx]._enc = encrypt_data(data.data);
        if(data.name) credentials[idx].name = data.name;
        if(data.type) credentials[idx].type = data.type;
        credentials[idx].updated_at = now;
        return json(res,200,masked(credentials[idx]));
      }

      if(pathname==='/api/validate'){
        try{
          const diags=mockValidate(data);
          return json(res,200,diags);
        }catch(e){ return json(res,400,{message:e.message}); }
      }
      if(pathname==='/api/explain'){
        try{
          const order=mockTopo(data);
          return json(res,200,order);
        }catch(e){ return text(res,400,e.message,'text/plain'); }
      }
      if(pathname==='/api/run'){
        try{
          const report=mockRun(data);
          const execId = 'exec-'+Date.now()+'-'+Math.random().toString(36).slice(2,6);
          const now = new Date().toISOString();
          const exec = {
            id: execId,
            workflow_id: data.id||'manual',
            workflow_name: data.name||'My workflow',
            status: 'success',
            started_at: now,
            finished_at: now,
            data: report,
            mode: 'manual',
          };
          runs.push({id:nextId++, at_epoch:Math.floor(Date.now()/1000), workflow:data.name||"unnamed", ok:true, order:report.order, total_ms:report.total_ms, execution_id: execId});
          executions.push(exec);
          if(runs.length>200) runs.shift();
          if(executions.length>200) executions.shift();
          report.execution_id = execId;
          return json(res,200,report);
        }catch(e){ return text(res,400,e.message,'text/plain'); }
      }
      if(pathname.startsWith('/api/wait/resume/')){
        const id = pathname.replace('/api/wait/resume/','').split('/')[0];
        // mock resume: find execution and mark resumed
        const exec = executions.find(e=>e.id===id || e.id.includes(id));
        if(!exec) return json(res,200,{resumed:true, id, message:'wait resumed (mock)', note:'marker POST /api/wait/resume/{id}'});
        exec.status='success';
        exec.finished_at=new Date().toISOString();
        return json(res,200,{resumed:true, id: exec.id, status: exec.status});
      }
      if(pathname==='/api/hooks'){
        const p = (data.path||'').trim().replace(/^\/+|\/+$/g,'');
        if(!p || p.includes('/') || p.length>64) return text(res,400,'hook path harus satu segmen','text/plain');
        hooks.set(p, data.workflow||{name:p, nodes:[], connections:{}});
        return json(res,201,p);
      }
      return text(res,404,'not found','text/plain');
    });
    return;
  }

  if(pathname.startsWith('/hook/')){
    const p=pathname.replace('/hook/','').split('/')[0];
    const wf=hooks.get(p);
    if(!wf) return text(res,404,'hook tak dikenal: '+p,'text/plain');
    try{
      const report=mockRun(wf);
      const execId = 'exec-'+Date.now()+'-'+Math.random().toString(36).slice(2,6);
      const now = new Date().toISOString();
      runs.push({id:nextId++, at_epoch:Math.floor(Date.now()/1000), workflow:wf.name||p, ok:true, order:report.order, total_ms:report.total_ms, execution_id: execId});
      executions.push({id:execId, workflow_id: wf.id||p, workflow_name: wf.name||p, status:'success', started_at:now, finished_at:now, data:report});
      if(runs.length>200) runs.shift();
      if(executions.length>200) executions.shift();
      const mode=wf.nodes?.find(n=>n.type==='n8n-nodes-base.webhook')?.parameters?.responseMode||'onReceived';
      if(mode==='onReceived') return json(res,200,{...report, execution_id:execId});
      if(mode==='lastNode'){
        const last=report.order[report.order.length-1];
        const items=report.outputs[last]?.[0]||[];
        return json(res,200,items[0]||null);
      }
      return json(res,200,{...report, execution_id:execId});
    }catch(e){
      return text(res,400,e.message,'text/plain');
    }
  }

  return text(res,404,'not found','text/plain');
});

server.listen(PORT, HOST, ()=>{
  console.log(`n8n-rust premium preview running at http://${HOST}:${PORT}`);
  console.log(`UI: http://${HOST}:${PORT}/`);
  console.log(`API: /health /api/nodes /api/validate /api/explain /api/run /api/runs /api/workflows CRUD /api/credentials CRUD + credential-types /api/executions /api/wait/resume/{id} /api/hooks /hook/{path} /api/metrics`);
});
