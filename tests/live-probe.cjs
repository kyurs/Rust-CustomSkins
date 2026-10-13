if(!process.env.PROBE_HOST)throw Error('Set PROBE_HOST to an authorized test server');
// Connect only disposable offline test accounts. Never uses a real player's login.
const mc=require('minecraft-protocol');
const assert=require('node:assert/strict');
const {setTimeout:delay}=require('node:timers/promises');
const clients=[];
const host=process.env.PROBE_HOST;
const plain=o=>typeof o==='string'?o:Array.isArray(o)?o.map(plain).join(''):(o?.text||o?.translate||'')+(o?.with||[]).map(plain).join('')+(o?.extra||[]).map(plain).join('');
async function until(fn,label,ms=40000){const end=Date.now()+ms;while(Date.now()<end){if(fn())return;await delay(100);}throw Error('Timed out: '+label);}
async function connectOnce(username){
 const c=mc.createClient({host,port:Number(process.env.PROBE_PORT||25565),username,auth:'offline',version:process.env.PROBE_VERSION||'1.12.2'});
 const p={c,messages:[],profiles:new Map(),ready:false,ended:false};clients.push(p);
 c.on('error',e=>p.error=e);
 c.on('end',()=>p.ended=true);
 c.on('packet',(v,m)=>{
  if(m.name==='position'){c.write('teleport_confirm',{teleportId:v.teleportId});p.ready=true;}
  if(m.name==='chat'){const message=plain(JSON.parse(v.message));p.messages.push(message);if(process.env.PROBE_DEBUG)console.log(username,message);}
  if(m.name==='kick_disconnect'){p.kick=plain(JSON.parse(v.reason));}
  if(m.name==='player_info'&&v.action==='add_player')for(const row of v.data){if(row.name)p.profiles.set(row.name,row.properties);}
 });
 c.once('login',()=>c.write('settings',{locale:'en_US',viewDistance:2,chatFlags:0,chatColors:true,skinParts:127,mainHand:1}));
 await until(()=>p.ready||p.error||p.ended,username+' join');if(p.error)throw p.error;if(!p.ready)return null;assert(p.ready,username+' joined: '+p.kick);return p;
}
async function connect(username){for(let i=0;i<8;i++){const p=await connectOnce(username);if(p)return p;await delay(2500);}throw Error('Skin lookup did not finish for '+username);}
function command(p,s){p.messages=[];p.c.write('chat',{message:s});}
function skin(p,name){const props=p.profiles.get(name)||[];const t=props.find(x=>x.name==='textures');assert(t,'texture property for '+name);assert(t.signature,'signed texture');return JSON.parse(Buffer.from(t.value,'base64').toString());}
(async()=>{try{
 const admin=await connect('SkinAdminProbe');await delay(2500);
 let member=await connect('SkinProbe');
 command(member,'/customskins moderate SkinAdminProbe steve');
 await until(()=>member.messages.some(s=>s.includes('Only level-4 Pumpkin operators')),'deny member moderation');
 console.log('PASS: ordinary player cannot moderate others');
 command(member,'/custom skins set MHF_Alex');
 await until(()=>member.ended,'self skin change reconnect');await delay(2500);
 member=await connect('SkinProbe');
 await until(()=>member.profiles.has('SkinProbe'),'own profile');
 assert.equal(skin(member,'SkinProbe').profileName,'MHF_Alex');
 console.log('PASS: spaced command, official signed skin and reconnect persistence');
 command(admin,'/customskins moderate SkinProbe steve');
 await until(()=>member.ended,'moderation reconnect');await delay(2500);
 member=await connect('SkinProbe');await until(()=>member.profiles.has('SkinProbe'),'moderated profile');
 assert.equal(skin(member,'SkinProbe').profileName,'MHF_Steve');
 command(member,'/customskins status');await until(()=>member.messages.some(s=>s.includes('moderated to steve')),'moderation status');
 await delay(16000);command(member,'/customskins set MHF_Alex');await until(()=>member.messages.some(s=>s.includes('must unmoderate')),'moderation lock');
 console.log('PASS: operator moderation enforces Steve and blocks self changes');
 command(admin,'/customskins unmoderate SkinProbe');await until(()=>member.ended,'unmoderate');await delay(2500);
 member=await connect('SkinProbe');command(member,'/customskins status');await until(()=>member.messages.some(s=>s.includes('automatic official lookup')),'automatic mode restored');
 console.log('PASS: unmoderate restores automatic lookup');
 member.c.end();member.c.socket?.destroy();await delay(2500);
 const official=await connect('MHF_Alex');await until(()=>official.profiles.has('MHF_Alex'),'automatic official profile');assert.equal(skin(official,'MHF_Alex').profileName,'MHF_Alex');
 console.log('PASS: automatic Mojang lookup on login after asynchronous prefetch');
}finally{for(const p of clients){p.c.end();p.c.socket?.destroy();}}})().then(()=>process.exit(0)).catch(e=>{console.error(e);process.exit(1);});
