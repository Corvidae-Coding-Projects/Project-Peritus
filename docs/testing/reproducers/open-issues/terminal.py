"""Observe and operate an actual native application PTY, with independent terminal emulation."""
import codecs, fcntl, os, pty, select, struct, subprocess, termios, time
import pyte

class Terminal:
    def __init__(self, command, env, output, width=160, height=48):
        self.output=output;self.raw=bytearray()
        self.master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',height,width,0,0))
        self.process=subprocess.Popen([str(value) for value in command],env=env,stdin=slave,stdout=slave,stderr=slave)
        os.close(slave)
        self.screen=pyte.Screen(width,height);self.stream=pyte.Stream(self.screen);self.decoder=codecs.getincrementaldecoder('utf-8')('replace')
    @property
    def text(self):return '\n'.join(self.screen.display)
    def send(self,value):os.write(self.master,value.encode() if isinstance(value,str) else value)
    def command(self,value):self.send(value+'\r')
    def pump(self,seconds=.1):
        deadline=time.monotonic()+seconds
        while time.monotonic()<deadline:
            if select.select([self.master],[],[],.05)[0]:
                try:data=os.read(self.master,65536)
                except OSError:return
                self.raw.extend(data);self.stream.feed(self.decoder.decode(data))
                if b'\x1b[6n' in data:self.send(b'\x1b[1;1R')
                if b'\x1b[?u' in data:self.send(b'\x1b[?1;2c')
    def wait(self,predicate,label,timeout=90):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            self.pump()
            if predicate(self.text):
                self.output.with_name(self.output.name+'-'+label+'.screen').write_text(self.text)
                return self.text
            assert self.process.poll() is None,(label,self.text)
        raise AssertionError(label+'\n'+self.text)
    def close(self,graceful=True):
        if graceful and self.process.poll() is None:
            self.send(b'\x11')
            try:self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:pass
        if self.process.poll() is None:self.process.terminate();self.process.wait(timeout=5)
        self.output.with_suffix('.ansi').write_bytes(self.raw)
        os.close(self.master)
