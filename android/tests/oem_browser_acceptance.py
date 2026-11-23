"""Physical Android browser acceptance when the OEM does not expose HTML nodes.

Serve only the bundled test form over adb reverse. Web state is an independent
oracle. All IME actions are real touches. Only synthetic fixture results are saved.
"""
import argparse
import json
import re
import secrets
import struct
import subprocess
import threading
import time
import xml.etree.ElementTree as ET
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

FORM = Path(__file__).with_name("host.html")
BROWSER = "com.heytap.browser"
IME = "org.mywubi"
HOST_PORT = 18767
PHONE_PORT = 8767
BRIDGE = r"""<script>(() => {
const fields=[...document.querySelectorAll('input,textarea')];
const ticket=location.pathname.split('/')[1];
const marker=document.createElement('div');
marker.style.cssText='position:fixed;width:14px;height:14px;background:rgb(11,253,131);z-index:99999;pointer-events:none';
document.body.append(marker);
let mark=fields.findIndex(f=>!f.closest('label').hidden);
function markField(){
 const r=fields[mark].getBoundingClientRect();
 marker.style.left=(r.left+r.width/2-7)+'px';marker.style.top=(r.top+r.height/2-7)+'px';
}
async function tick(){
 try{
  markField();
  const state=fields.map(f=>({v:f.type==='password'?'':f.value,length:f.value.length,focus:document.activeElement===f,
   start:f.selectionStart,end:f.selectionEnd}));
  const res=await fetch('/'+ticket+'/state',{method:'POST',headers:{'Content-Type':'application/json'},
    body:JSON.stringify({run,idx:selected,submits:window.submits,fields:state})});
  const cmd=await res.json();
  if(cmd.op==='set'){const f=fields[cmd.index];f.value=cmd.value;f.focus();f.setSelectionRange(cmd.start,cmd.end);f.dispatchEvent(new Event('input',{bubbles:true}));}
  if(cmd.op==='mark'){mark=cmd.index;fields[mark].scrollIntoView({block:'center'});markField();}
 }catch{}
 setTimeout(tick,100);
}
tick();
})();</script>"""
class Fixture:
    def __init__(self):
        self.ticket=secrets.token_hex(16)
        self.run=''
        self.state={}
        self.cmd={}
        self.lock=threading.Lock()
        outer=self
        class Request(BaseHTTPRequestHandler):
            def log_message(self,*args): pass
            def reply(self,body,content_type):
                self.send_response(200)
                self.send_header('Content-Type',content_type)
                self.send_header('Content-Length',str(len(body)))
                self.send_header('Cache-Control','no-store')
                self.end_headers()
                self.wfile.write(body)
            def do_GET(self):
                if urlsplit(self.path).path != f'/{outer.ticket}/host.html':
                    self.send_error(404)
                    return
                self.reply((FORM.read_text(encoding='utf8').replace('</html>',BRIDGE+'</html>')).encode(),'text/html; charset=utf-8')
            def do_POST(self):
                if self.path!=f'/{outer.ticket}/state' or not (0<int(self.headers.get('Content-Length',0))<16000):
                    self.send_error(400)
                    return
                state=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                with outer.lock:
                    command={}
                    if state.get('run')==outer.run:
                        outer.state=state
                        command,outer.cmd=outer.cmd,{}
                self.reply(json.dumps(command).encode(),'application/json')
        self.server=ThreadingHTTPServer(('127.0.0.1',HOST_PORT),Request)
        threading.Thread(target=self.server.serve_forever,daemon=True).start()
    def prepare(self,idx):
        with self.lock:
            self.run=str(time.monotonic_ns())
            self.state={}
            self.cmd={}
        return f'http://127.0.0.1:{PHONE_PORT}/{self.ticket}/host.html?case={idx}-{self.run}'
    def command(self,**cmd):
        with self.lock:self.cmd=cmd
    def wait(self,test,message,timeout=9):
        end=time.monotonic()+timeout
        while time.monotonic()<end:
            with self.lock:
                if self.state and test(self.state):return
            time.sleep(.08)
        raise AssertionError(message)
class Phone:
    def __init__(self,serial,fixture):
        self.adb=['adb','-s',serial]
        self.fixture=fixture
        self.steps=[]
    def raw(self,*args):return subprocess.check_output(self.adb+list(args),timeout=20)
    def shell(self,*args):return self.raw('shell',*map(str,args)).decode('utf8','replace').strip()
    def nodes(self):
        xml=self.raw('exec-out','uiautomator','dump','--windows','/dev/tty')
        a,b=xml.find(b'<displays'),xml.find(b'</displays>')
        if a<0 or b<0:return []
        return [n.attrib for n in ET.fromstring(xml[a:b+11]).iter('node')]
    def node(self,match,reason,timeout=8):
        end=time.monotonic()+timeout
        while time.monotonic()<end:
            found=next((n for n in self.nodes() if match(n)),None)
            if found:return found
        raise AssertionError(reason)
    def tap(self,n):
        l,t,r,b=map(int,re.findall(r'\d+',n['bounds']))
        self.shell('input','tap',(l+r)//2,(t+b)//2)
    def key(self,label):
        n=self.node(lambda n:n['package']==IME and n['content-desc']==label and n['enabled']=='true','缺少可用按键 '+label)
        self.tap(n)
    def candidate(self,word):
        self.tap(self.node(lambda n:n['package']==IME and n['content-desc'].split('，')[0]=='候选 '+word,'缺少候选 '+word))
    def touch_field(self,index):
        self.fixture.command(op='mark',index=index)
        time.sleep(.45)
        end=time.monotonic()+7
        while time.monotonic()<end:
            window=self.shell('dumpsys','window')
            if any('mCurrentFocus=' in line and BROWSER+'/' in line for line in window.splitlines()):break
            time.sleep(.15)
        else:raise AssertionError('浏览器窗口没有焦点')
        while time.monotonic()<end:
            raw=self.raw('exec-out','screencap')
            w,h,fmt=struct.unpack_from('<III',raw)
            assert fmt in (1,2) and 0<len(raw)-w*h*4<=32,'未知屏幕像素格式'
            data=raw[len(raw)-w*h*4:]
            offset=data.find(bytes((11,253,131,255))*8)
            if offset>=0:break
            time.sleep(.12)
        else:raise AssertionError('本地验收表单的点击标记不可见')
        pixel=offset//4
        self.shell('input','tap',pixel%w+4,pixel//w+3)
        self.fixture.wait(lambda s:s['fields'][index]['focus'],f'第 {index} 个真实输入框未获得焦点')
    def open(self,index,focus=True):
        url=self.fixture.prepare(index)
        self.shell('am','start','-W','-a','android.intent.action.VIEW','-d',url,'-p',BROWSER)
        self.fixture.wait(lambda s:s['idx']==str(index),'本地验收页没有报告当前场景',15)
        if focus:self.touch_field(index)
    def expect(self,index,wanted):
        self.fixture.wait(lambda s:s['fields'][index]['v']==wanted,f'正文未变为 {wanted!r}')
        self.steps.append('正文 '+repr(wanted))
    def count(self,count):
        self.fixture.wait(lambda s:s['submits']==count,f'提交计数未变为 {count}')
    def set(self,value,start,end):
        self.fixture.command(op='set',index=0,value=value,start=start,end=end)
        self.fixture.wait(lambda s:s['fields'][0]['v']==value and s['fields'][0]['start']==start and s['fields'][0]['end']==end,'未建立测试选区')
    def main_mode(self):
        for n in self.nodes():
            if n['package']==IME and n['content-desc']=='英 → 中':self.tap(n);break
    def address(self):
        self.open(0,focus=False)
        self.tap(self.node(lambda n:n['package']==BROWSER and n['resource-id']==BROWSER+':id/web_title' and n['text']=='MyWubi 浏览器输入验收','找不到验收网页标题栏'))
        def address_node(n):return n['package']==BROWSER and n['class'].endswith(('EditText','AutoCompleteTextView')) and n['focused']=='true'
        self.node(address_node,'地址栏没有获得焦点')
        self.shell('input','keycombination','113','29')
        self.shell('input','keyevent','67')
        self.main_mode()
        self.key('p');self.candidate('这')
        self.node(lambda n:address_node(n) and n['text']=='这','地址栏未收到“这”')
        self.key('中 → 英')
        try:
            self.key('p');self.key('.')
            if not any(n['package']==IME and n['content-desc']=='/' for n in self.nodes()):
                self.key('符号')
            self.key('/')
            self.node(lambda n:address_node(n) and n['text']=='这p./','混输后焦点或地址正文不符')
        finally:
            if any(n['package']==IME and n['content-desc']=='ABC' for n in self.nodes()):self.key('ABC')
            if any(n['package']==IME and n['content-desc']=='英 → 中' for n in self.nodes()):self.key('英 → 中')
    def enter(self):
        self.open(1);self.main_mode()
        for k in ['拼音','空格','发送']:self.key(k)
        self.count(0)
        for k in ['n','i','发送']:self.key(k)
        self.expect(1,'ni');self.count(0);self.key('发送');self.count(1)
        self.open(2);self.key('g');self.key('搜索');self.expect(2,'g');self.count(0)
        self.key('搜索');self.count(1)
        self.open(0);self.key('g');self.key('换行');self.expect(0,'g');self.key('换行');self.expect(0,'g\n')
    def unicode(self):
        self.open(0);self.main_mode()
        for cluster in ['汉','😀','e\u0301','👩‍💻','🇨🇳','👍🏽']:
            value='前'+cluster;end=len(value.encode('utf-16-le'))//2
            self.set(value,end,end);self.key('退格');self.expect(0,'前')
        self.set('甲😀e\u0301乙',0,6);self.key('退格');self.expect(0,'')
        self.key('g');self.key('g');self.expect(0,'gg');self.shell('input','keyevent','21')
        self.key('退格');self.expect(0,'g');self.key('a');self.expect(0,'ag');self.key('退格');self.expect(0,'g')
    def rotation(self):
        self.open(0);self.main_mode();self.key('g');self.expect(0,'g')
        self.shell('input','keyevent',4);self.touch_field(0);self.key('g');self.expect(0,'gg')
        self.shell('wm','user-rotation','lock',1)
        try:
            time.sleep(1.2)
            w,h=struct.unpack_from('<II',self.raw('exec-out','screencap'))
            assert w>h,'手机并未进入横屏'
            if not self.fixture.state['fields'][0]['focus'] or 'mInputShown=true' not in self.shell('dumpsys','input_method'):
                self.touch_field(0)
            self.shell('input','keyevent',123);self.key('换行')
            self.fixture.wait(lambda s:s['fields'][0]['v'] in ('gg','gg\n'),'旋转后出现正文丢失或重复')
        finally:
            self.shell('wm','user-rotation','lock',0);time.sleep(.5);self.shell('wm','user-rotation','free')
        self.open(8);self.key('g');self.shell('input','keyevent',4);self.touch_field(9);self.key('a');self.expect(9,'a')
        self.shell('input','keyevent',3);self.shell('am','start','-W','-a','android.settings.SETTINGS')
        self.open(0);self.key('g');self.expect(0,'g')
    def restricted(self):
        errors=[]
        for index,keys,expected in [(3,'ggll',None),(4,'12','12'),(5,'+12','+12'),(6,'/.','/.'),(7,'a@','a@'),(8,['下一项'],None)]:
            try:
                self.open(index)
                for label in keys:self.key(label)
                if expected is not None:self.expect(index,expected)
                if index==3:
                    self.fixture.wait(lambda s:s['fields'][3]['length']==4,'密码长度不符')
                    visible=self.nodes()
                    assert any(n['package']==IME and n['content-desc']=='安全输入' for n in visible),'缺少安全输入标识'
                    assert not any(n['package']==IME and (n['content-desc'].startswith('候选 ') or n['text']=='ggll') for n in visible),'敏感字段有候选或明文'
                if index==4:assert not any(n['package']==IME and n['content-desc'].startswith('候选 ') for n in self.nodes()),'数字字段有候选'
                if index==8:self.fixture.wait(lambda s:s['fields'][9]['focus'],'下一项没有移动焦点')
                self.steps.append('字段 '+str(index)+' 通过')
            except (AssertionError, subprocess.SubprocessError) as exc:
                errors.append(f'{index}: {exc}')
                if index==3:
                    self.steps.append('密码场景系统窗口包 '+', '.join(sorted({n['package'] for n in self.nodes() if n.get('package')})))
        if errors:raise AssertionError(' / '.join(errors))
def main():
    p=argparse.ArgumentParser()
    p.add_argument('serial');p.add_argument('output',type=Path);p.add_argument('ids',type=int,nargs='+',choices=[1,2,3,5,7]);args=p.parse_args()
    f=Fixture();phone=Phone(args.serial,f)
    original=phone.shell('settings','get','secure','default_input_method')
    rotation=phone.shell('wm','user-rotation')
    services=phone.shell('settings','get','secure','enabled_accessibility_services')
    mapping={1:phone.address,2:phone.enter,3:phone.rotation,5:phone.restricted,7:phone.unicode}
    try:
        assert 'talkback' not in services.lower()
        reverse=phone.raw('reverse','--list').decode()
        if any(f'tcp:{PHONE_PORT} ' in x for x in reverse.splitlines()):raise RuntimeError('测试端口已有转发')
        phone.raw('reverse',f'tcp:{PHONE_PORT}',f'tcp:{HOST_PORT}')
        phone.shell('ime','enable',IME+'/.MyWubiService');phone.shell('ime','set',IME+'/.MyWubiService')
        for i in args.ids:
            phone.steps=[];result={'id':i,'driver':'ADB 实际触摸和本地 DOM 回读'}
            try:mapping[i]();result['result']='通过'
            except (AssertionError,subprocess.SubprocessError) as exc:result.update(result='失败',reason=str(exc))
            result['steps']=phone.steps
            with args.output.open('a',encoding='utf8') as log:log.write(json.dumps(result,ensure_ascii=False)+'\n')
            print(i,result['result'],result.get('reason',''),flush=True)
    finally:
        phone.shell('ime','set',original)
        if rotation.startswith('lock'):phone.shell('wm','user-rotation',*rotation.split())
        f.server.shutdown();f.server.server_close()
        if 'reverse' in locals() and f'tcp:{PHONE_PORT} ' not in reverse:phone.raw('reverse','--remove',f'tcp:{PHONE_PORT}')
if __name__=='__main__': main()
