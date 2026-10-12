"""Native Linux app accessibility, explicit X11 input, and owned test displays."""
import os
from pathlib import Path
import select
import shutil
import subprocess
import time
import uuid


def birth(pid):
    path = Path('/proc') / str(pid)
    if path.stat().st_uid != os.getuid():
        raise ValueError('The selected app is not owned by the current user')
    fields = (path / 'stat').read_text().rsplit(')', 1)[1].split()
    if fields[0] == 'Z':
        raise FileNotFoundError('The selected application has exited')
    return fields[19]


class Desktops:
    def __init__(self, root, workspace, screenshot):
        self.root = root
        self.workspace = workspace
        self.screenshot = screenshot
        self.sessions = {}
        self.atspi = None

    def accessibility(self):
        if self.atspi is None:
            if os.name != 'posix' or not Path('/proc').is_dir():
                raise ValueError('Native desktop control currently requires Linux AT-SPI or X11; browser control is separate')
            import gi
            gi.require_version('Atspi', '2.0')
            from gi.repository import Atspi
            Atspi.init()
            Atspi.set_timeout(3000, 5000)
            self.atspi = Atspi
        return self.atspi

    def apps(self):
        desktop = self.accessibility().get_desktop(0)
        desktop.clear_cache()
        result = []
        for index in range(desktop.get_child_count()):
            try:
                app = desktop.get_child_at_index(index)
                pid = app.get_process_id()
                result.append({'pid': pid, 'birth': birth(pid), 'name': app.get_name()})
            except Exception:
                continue
        return result

    def app(self, pid):
        desktop = self.accessibility().get_desktop(0)
        desktop.clear_cache()
        for index in range(desktop.get_child_count()):
            try:
                app = desktop.get_child_at_index(index)
                if app.get_process_id() == pid:
                    return app
            except Exception:
                continue
        return None

    def control(self, args):
        action = args['action']
        if action == 'list':
            return {'success': True, 'apps': self.apps()}
        if action in ('open', 'attach'):
            identifier = uuid.uuid4().hex
            directory = self.root / identifier
            directory.mkdir(mode=0o700)
            session = {'owned': action == 'open', 'directory': directory, 'refs': {}, 'display': os.environ.get('DISPLAY'), 'child': None, 'xvfb': None}
            try:
                if action == 'open':
                    env = os.environ.copy()
                    # Opt in for the child only; do not change the user's desktop-wide
                    # accessibility setting or retrofit unrelated running applications.
                    env['QT_LINUX_ACCESSIBILITY_ALWAYS_ON'] = '1'
                    env['NO_AT_BRIDGE'] = '0'
                    if args.get('isolated', False):
                        executable = shutil.which('Xvfb')
                        if executable is None:
                            raise ValueError('Isolated native displays require Xvfb')
                        read, write = os.pipe()
                        try:
                            session['xvfb'] = subprocess.Popen([executable, '-displayfd', str(write), '-screen', '0', '1280x900x24', '-nolisten', 'tcp'], pass_fds=(write,), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                            os.close(write)
                            write = None
                            if not select.select([read], [], [], 5)[0]:
                                raise ValueError('Isolated display did not become ready')
                            number = os.read(read, 32).decode().strip()
                            if not number.isdigit():
                                raise ValueError('Invalid isolated display identity')
                            session['display'] = ':' + number
                        finally:
                            os.close(read)
                            if write is not None:
                                os.close(write)
                        env.update(DISPLAY=session['display'], GDK_BACKEND='x11', QT_QPA_PLATFORM='xcb')
                        # Keep the child's menus on its private display instead of
                        # exporting them to the user's KDE global-menu service.
                        env['KDE_NO_GLOBAL_MENU'] = '1'
                        env.update(XDG_CONFIG_HOME=str(directory / 'config'),
                                   XDG_CACHE_HOME=str(directory / 'cache'),
                                   XDG_DATA_HOME=str(directory / 'data'))
                        env.pop('WAYLAND_DISPLAY', None)
                    with (directory / 'app.log').open('wb') as output:
                        session['child'] = subprocess.Popen([args['program'], *args.get('args', [])], cwd=self.workspace, env=env, stdout=output, stderr=output)
                    pid = session['child'].pid
                else:
                    pid = args['pid']
                session.update(pid=pid, birth=birth(pid))
                self.sessions[identifier] = session
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    if self.app(pid) or self.window(session):
                        break
                    time.sleep(0.1)
                return self.observe(identifier)
            except Exception:
                self.sessions.pop(identifier, None)
                self.stop_owned(session)
                raise
        identifier = args['session']
        session = self.sessions.get(identifier)
        if session is None:
            raise ValueError('Desktop session is unavailable; open or attach again')
        if action == 'close':
            self.close(identifier)
            return {'success': True, 'closed': identifier, 'existing_app_preserved': not session['owned']}
        if action == 'observe':
            return self.observe(identifier, args.get('ref'))
        if birth(session['pid']) != session['birth']:
            raise ValueError('Application birth identity changed; no action was sent')
        if args.get('observation') != session.get('observation'):
            raise ValueError('Stale or missing observation; observe the app before acting')
        node = session['refs'].get(args.get('ref'))
        if node is not None:
            if node.get_application().get_process_id() != session['pid'] or node.get_state_set().contains(self.accessibility().StateType.DEFUNCT):
                raise ValueError('Accessible reference no longer belongs to the selected live app')
        if action == 'fill':
            if node is None:
                raise ValueError('fill requires an observed editable text ref')
            editable = node.get_editable_text_iface()
            if editable is None or not editable.set_text_contents(args['text']):
                raise ValueError('The selected control did not accept editable text')
        elif action in ('click', 'invoke') and node is not None:
            interface = node.get_action_iface()
            index = args.get('index', 0)
            if interface is None or index < 0 or index >= interface.get_n_actions() or not interface.do_action(index):
                raise ValueError('This node has no usable native action; inspect its actions or use supported pixel input')
        elif action == 'focus' and node is not None:
            component = node.get_component_iface()
            if component is None or not component.grab_focus():
                raise ValueError('The selected control could not receive focus')
        elif action in ('click', 'press', 'type', 'scroll'):
            window = self.window(session)
            if window is None:
                raise ValueError('Pixel/keyboard input is unavailable for this Wayland window; use its observed native accessible actions or launch an isolated X11 test session')
            env = os.environ.copy()
            env['DISPLAY'] = session['display']
            self.run(['xdotool', 'windowfocus', '--sync', window], env)
            if action == 'click':
                geometry = self.geometry(window, env)
                x = int(args['x'] / session['scale'])
                y = int(args['y'] / session['scale'])
                if not 0 <= x < geometry['WIDTH'] or not 0 <= y < geometry['HEIGHT']:
                    raise ValueError('Coordinates are outside the selected window screenshot')
                # --sync can wait forever when the pointer is already at this position.
                # The subsequent observation supplies post-action evidence.
                self.run(['xdotool', 'mousemove', '--window', window, str(x), str(y), 'click', '1'], env)
            elif action == 'press':
                self.run(['xdotool', 'key', '--clearmodifiers', args['key']], env)
            elif action == 'type':
                # xdotool type does not emit Return for newline characters. Send the
                # actual keys between lines instead of silently dropping user text.
                lines = args['text'].replace('\r\n', '\n').replace('\r', '\n').split('\n')
                for index, line in enumerate(lines):
                    if index:
                        self.run(['xdotool', 'key', '--clearmodifiers', 'Return'], env)
                    if line:
                        self.run(['xdotool', 'type', '--delay', '0', '--clearmodifiers', '--', line], env)
            else:
                self.run(['xdotool', 'click', '--repeat', str(min(20, abs(args.get('amount', 3)))), '4' if args.get('amount', 3) < 0 else '5'], env)
        else:
            raise ValueError('Unsupported desktop action or missing observed ref')
        return self.observe(identifier)

    def run(self, arguments, env=None):
        return subprocess.run(arguments, env=env, capture_output=True, text=True, check=True, timeout=10).stdout

    def window(self, session):
        if not session['display'] or shutil.which('xdotool') is None:
            return None
        env = os.environ.copy()
        env['DISPLAY'] = session['display']
        result = subprocess.run(['xdotool', 'search', '--onlyvisible', '--pid', str(session['pid'])], env=env, capture_output=True, text=True, timeout=5)
        windows = result.stdout.splitlines()
        return windows[-1] if windows and result.returncode == 0 else None

    def geometry(self, window, env):
        values = self.run(['xdotool', 'getwindowgeometry', '--shell', window], env)
        return {key: int(value) for key, value in (line.split('=', 1) for line in values.splitlines())}

    def observe(self, identifier, root_ref=None):
        session = self.sessions[identifier]
        try:
            same = (session['child'] is None or session['child'].poll() is None) and birth(session['pid']) == session['birth']
        except FileNotFoundError:
            same = False
        if not same:
            session['refs'] = {}
            session['observation'] = uuid.uuid4().hex
            return {'success': True, 'kind': 'desktop', 'session': identifier,
                    'state': 'closed', 'pid': session['pid'], 'owned': session['owned'],
                    'observation': session['observation'], 'nodes': [], 'image': None,
                    'recovery': 'The selected process ended; open or explicitly attach a new app'}
        observation = uuid.uuid4().hex
        app = self.app(session['pid'])
        refs = {}
        nodes = []
        hidden_menu_branches = 0
        selected = session['refs'].get(root_ref) if root_ref else app
        if root_ref and selected is None:
            raise ValueError('Native subtree reference is unavailable; observe the complete app again')
        queue = [(selected, None, 0)] if selected else []
        while queue and len(nodes) < 500:
            node, parent, depth = queue.pop(0)
            try:
                states = node.get_state_set()
                role = node.get_role_name()
                # Qt can report SHOWING=false even for an open popup. Keep a
                # discoverable branch instead of treating that flag as proof that
                # its controls are absent. Explicitly scoping that branch expands it.
                collapsed = (role in ('menu', 'popup menu')
                             and not states.contains(self.accessibility().StateType.SHOWING)
                             and not (root_ref is not None and depth == 0))
                ref = 'n' + str(len(refs) + 1)
                refs[ref] = node
                item = {'ref': ref, 'parent': parent, 'role': role, 'name': (node.get_name() or '')[:1000], 'enabled': states.contains(self.accessibility().StateType.ENABLED) or states.contains(self.accessibility().StateType.SENSITIVE), 'showing': states.contains(self.accessibility().StateType.SHOWING), 'editable': states.contains(self.accessibility().StateType.EDITABLE)}
                try:
                    text = node.get_text_iface()
                    if text is not None and node.get_role() != self.accessibility().Role.PASSWORD_TEXT:
                        # GI exposes both Accessible.get_text() (interface lookup) and
                        # Text.get_text(start, end) on the same proxy. Dispatch explicitly.
                        api = self.accessibility().Text
                        item['text'] = api.get_text(text, 0, min(2000, api.get_character_count(text)))
                except Exception as error:
                    item['text_unavailable'] = str(error)[:200]
                try:
                    action = node.get_action_iface()
                    if action:
                        item['actions'] = [action.get_action_name(i) for i in range(action.get_n_actions())]
                except Exception:
                    pass
                nodes.append(item)
                if collapsed:
                    hidden_menu_branches += 1
                    item['children_omitted'] = True
                    item['child_count'] = node.get_child_count()
                    item['child_names_preview'] = [(node.get_child_at_index(index).get_name() or '')[:256]
                                                   for index in range(min(4, item['child_count']))]
                    item['recovery'] = 'Menu visibility flags may be unreliable. If this is the open menu, snapshot this ref to inspect its controls.'
                elif depth < 12:
                    for index in range(min(100, node.get_child_count())):
                        child = node.get_child_at_index(index)
                        if child:
                            queue.append((child, ref, depth + 1))
            except Exception:
                continue
        raw = session['directory'] / ('raw-' + observation + '.png')
        image = None
        image_error = None
        window = self.window(session)
        try:
            if window:
                session['x11_seen'] = True
                env = os.environ.copy()
                env['DISPLAY'] = session['display']
                self.run(['import', '-window', window, str(raw)], env)
            elif app and app.get_child_count():
                if session.get('x11_seen') or session['display'] != os.environ.get('DISPLAY'):
                    raise ValueError('Selected X11 window is unavailable; no host-desktop capture was attempted')
                frame = app.get_child_at_index(0)
                component = frame.get_component_iface()
                if component is None or not frame.get_state_set().contains(self.accessibility().StateType.ACTIVE):
                    raise ValueError('Selected Wayland window is not active; focus it explicitly before requesting a screenshot')
                before = component.get_extents(self.accessibility().CoordType.SCREEN)
                self.run(['spectacle', '--background', '--nonotify', '--activewindow', '--output', str(raw)])
                # Active-window capture races with app closure/focus changes. Reject an
                # unrelated capture instead of delivering another app's pixels.
                frame.clear_cache()
                if birth(session['pid']) != session['birth'] or not frame.get_state_set().contains(self.accessibility().StateType.ACTIVE) or not frame.get_state_set().contains(self.accessibility().StateType.SHOWING):
                    raise ValueError('Selected Wayland window closed or lost focus during capture')
                from PIL import Image
                with Image.open(raw) as captured:
                    if abs(captured.width - before.width) > 64 or abs(captured.height - before.height) > 64:
                        raise ValueError('Active-window capture dimensions do not match the selected Wayland app')
            else:
                raise ValueError('No accessible window or X11 window is available')
            image = self.screenshot(raw, session['directory'] / (observation + '.png'))
        except Exception as error:
            raw.unlink(missing_ok=True)
            image_error = str(error)[:1000]
        session.update(refs=refs, observation=observation, scale=image['scale'] if image else 1)
        try:
            name = app.get_name() if app else None
        except Exception:
            name = None
        try:
            ended = birth(session['pid']) != session['birth']
        except FileNotFoundError:
            ended = True
        if ended:
            if image:
                Path(image['path']).unlink(missing_ok=True)
            return self.observe(identifier)
        return {'success': True, 'kind': 'desktop', 'session': identifier, 'observation': observation,
                'pid': session['pid'], 'birth': session['birth'], 'owned': session['owned'],
                'name': name, 'nodes': nodes, 'nodes_omitted': bool(queue),
                'collapsed_menu_branches': hidden_menu_branches,
                'image': image, 'image_unavailable': image_error,
                'pixel_input': window is not None, 'accessible_actions': app is not None}

    def close(self, identifier):
        session = self.sessions.pop(identifier)
        self.stop_owned(session)

    def stop_owned(self, session):
        for child in (session['child'], session['xvfb']):
            if child and child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()

    def shutdown(self):
        for identifier in list(self.sessions):
            self.close(identifier)
