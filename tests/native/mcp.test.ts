import { test, expect } from "bun:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { mkdtemp, rm, access } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

test.skipIf(!process.env.OMOCHI_NATIVE_TEST)("native MCP works through native gateway and hub, cancels reads and retracts on EOF", async () => {
  const root = await mkdtemp(join(tmpdir(), "omochi-mcp-"));
  const profile = process.env.OMOCHI_NATIVE_PROFILE ?? "debug";
  if (profile !== "debug" && profile !== "release") throw new Error("invalid native profile");
  const binary = resolve(import.meta.dir, "../../crates/target", profile);
  const hub = spawn(join(binary, "omochi-ui"), ["--config-dir", join(root,"config"), "--runtime-dir", join(root,"omochi"), "hub", "--port", "0"], {env:{...process.env,PATH:"/nonexistent"},stdio:["ignore","ignore","pipe"]});
  const transport = new StdioClientTransport({
    command:join(binary,"omochi-utils"),args:["mcp","--service-id","agent","--runtime-dir",join(root,"omochi")],
    env:{...process.env as Record<string,string>,PATH:"/nonexistent"},stderr:"pipe"
  });
  const client = new Client({name:"native-integration",version:"1"});
  let socket: WebSocket | undefined;
  const wait = async (predicate:()=>Promise<boolean>|boolean) => {
    const deadline = Date.now()+5000;
    while (!await predicate()) { if(Date.now()>deadline) throw new Error("condition timed out"); await Bun.sleep(10); }
  };
  try {
    const line = await new Promise<string>((ok,fail)=>{
      const reader = createInterface({input:hub.stderr!});
      reader.once("line",line=>{reader.close();ok(line)});
      hub.once("error",fail);
    });
    const base = line.replace("[omochi-ui] dashboard ","").trim();
    await client.connect(transport);
    expect((await client.listTools()).tools.map(t=>t.name)).toEqual(["publish_messages","list_surfaces","read_events"]);
    const call = (name:string,args:Record<string,unknown>={}) => client.callTool({name,arguments:args});
    const create = (id:string) => ({version:"v0.9",createSurface:{surfaceId:id,catalogId:"test",sendDataModel:true}});
    expect((await call("publish_messages",{messages:[create("main"),{version:"v0.9",updateDataModel:{surfaceId:"main",value:{title:"first"}}}]})).isError).toBeUndefined();
    expect((await call("list_surfaces")).structuredContent).toEqual({serviceId:"agent",surfaceIds:["main"]});
    expect((await call("publish_messages",{messages:[create("other"),{version:"v0.9",updateDataModel:{surfaceId:"absent",value:{}}}]})).isError).toBe(true);
    expect((await call("list_surfaces")).structuredContent).toEqual({serviceId:"agent",surfaceIds:["main"]});
    const messages: any[] = [];
    socket = new WebSocket(new URL("/ws",base).toString().replace("http:","ws:"));
    socket.addEventListener("message",event=>messages.push(JSON.parse(String(event.data))));
    await new Promise<void>((ok,fail)=>{socket!.addEventListener("open",()=>ok(),{once:true});socket!.addEventListener("error",fail,{once:true})});
    await wait(()=>messages.some(m=>m.createSurface));
    const surface = messages.find(m=>m.createSurface).createSurface.surfaceId;
    expect(surface).toMatch(/^[0-9a-f]{64}~agent\/main$/);
    await wait(()=>messages.some(m=>m.updateDataModel?.value?.title==="first"));
    const waiting = call("read_events",{after:0,timeoutMs:2000});
    socket.send(JSON.stringify({version:"v0.9",action:{surfaceId:surface,name:"submit",context:{answer:"yes"}}}));
    const received = (await waiting).structuredContent as any;
    expect(received.nextCursor).toBe(1);
    expect(received.events[0].message.action.surfaceId).toBe("main");
    expect((await call("read_events")).structuredContent).toEqual(received);
    const abort = new AbortController();
    const cancelled = client.callTool({name:"read_events",arguments:{after:1,timeoutMs:30000}},undefined,{signal:abort.signal});
    abort.abort();
    await expect(cancelled).rejects.toThrow();
    expect((await call("list_surfaces")).structuredContent).toEqual({serviceId:"agent",surfaceIds:["main"]});
    await call("publish_messages",{messages:[{version:"v0.9",deleteSurface:{surfaceId:"main"}}]});
    await wait(()=>messages.some(m=>m.deleteSurface?.surfaceId===surface));
    await call("publish_messages",{messages:[create("last")]});
    await wait(()=>messages.some(m=>m.createSurface?.surfaceId.endsWith("/last")));
    const last = messages.find(m=>m.createSurface?.surfaceId.endsWith("/last")).createSurface.surfaceId;
    await client.close();
    await wait(()=>messages.some(m=>m.deleteSurface?.surfaceId===last));
    await wait(async()=>{try {await access(join(root,"omochi","agent","manifest.json"));return false;}catch{return true;}});
  } finally {
    socket?.close(); await client.close().catch(()=>{}); hub.kill("SIGTERM");
    await new Promise<void>(ok=>{if(hub.exitCode!==null)ok();else hub.once("exit",()=>ok())});
    await rm(root,{recursive:true,force:true});
  }
},15000);
