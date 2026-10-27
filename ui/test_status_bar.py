# Copyright (c) 2026 Lovecast Inc.
# Copyright (c) 2026 Omar Garcia
# SPDX-License-Identifier: MIT
import json
import os
import re
from pathlib import Path
import subprocess
import time
from datetime import datetime, timezone
from urllib.parse import urlencode


def luminance(color):
    color = color.strip()
    values = [int(color[i:i+2], 16) for i in (1, 3, 5)] if color.startswith('#') else list(map(float, re.findall(r'[\d.]+', color)[:3]))
    return sum(weight * (value / 3294.6 if value <= 10.31475 else ((value / 255 + .055) / 1.055) ** 2.4) for weight, value in zip((.2126, .7152, .0722), values))


def verify_status_bar(test, browser, evaluate, ipc):
    shots = Path('/tmp/orquesta-ui-shots')
    shots.mkdir(parents=True, exist_ok=True)
    rpc = lambda action: ipc(dict(command='status-bar', action=action))['result']
    test.assertTrue(evaluate("document.querySelector('#sb-refresh').disabled"))
    test.assertIn('unavailable', evaluate("document.querySelector('#sb-terminals').ariaLabel"))
    rpc(dict(action='configure', id='local', name='Local decision'))
    rpc(dict(action='display', mode='compact'))
    entries = rpc(dict(action='snapshot'))['snapshot']['providers']
    expected = ['Claude', 'Codex', 'Gemini', 'Cursor', 'Grok', 'Kimi', 'MiniMax', 'Antigravity', 'OpenCode Go', 'ZCode', 'Local decision']
    test.assertEqual([p['name'] for p in entries], expected)
    fixture_file = Path(os.environ['ORQUESTA_USAGE_FIXTURES'])
    reset = datetime.fromtimestamp(time.time() + 7800, timezone.utc).isoformat().replace('+00:00', 'Z')
    windows = lambda index, value: [dict(label=label, percent_used=0 if index == 0 else max(0, value - offset), reset_in_seconds=7800, reset_at_iso=reset) for label, offset in [('5h', 0), ('wk', 10)]]
    fixture_file.write_text(json.dumps({p['id']: dict(phase='ready', windows=windows(i, 42), plan='Pro', delay=.2) for i, p in enumerate(entries)}))
    browser('fill', '#draft', 'Keep this draft\nunsent and selected.')
    evaluate("document.querySelector('#draft').setSelectionRange(5, 20, 'backward')")
    draft = "[draft.value,draft.selectionStart,draft.selectionEnd,draft.selectionDirection,submissions.value]"
    before = evaluate(draft)
    reference = subprocess.Popen(['node', 'ui/reference.mjs']) if os.environ.get('ORCA_REFERENCE_ROOT') else None
    ref = lambda *args: subprocess.check_output(['npx', '--yes', 'agent-browser@0.39.4', '--session', f'status-ref-{os.getpid()}', *args], text=True)
    gallery = []

    def install(phase, percent):
        if phase != 'loading':
            for index, entry in enumerate(entries):
                stages = ['ready'] if phase == 'ready' else ['ready', phase]
                for stage in stages:
                    claim = rpc(dict(action='begin', id=entry['id'], request_id=f'{time.time_ns()}-{index}-{stage}'))
                    if stage != 'refreshing':
                        rpc(dict(action='finish', id=entry['id'], token=claim['token'], now=int(time.time()) - 300, phase=stage, plan='Pro', windows=[] if phase == 'error_no_cache' else windows(index, percent)))
        tone = 'urgent' if percent >= 80 else 'warning' if percent >= 60 else 'normal'
        browser('wait', '--fn', f"document.querySelectorAll('.sb-provider').length === 11 && [...document.querySelectorAll('.sb-provider')].every(b=>b.dataset.phase === {json.dumps(phase)}) && document.querySelector('[data-id=codex]').dataset.tone === {json.dumps(tone)}")

    def capture(name, view, width, theme):
        browser('screenshot', str(shots / f'{name}.png'))
        test.assertTrue(evaluate(f"r = document.querySelector('#status-bar').getBoundingClientRect(); r.bottom === innerHeight && r.height === {44 if width == 375 else 24} && document.documentElement.scrollWidth <= innerWidth && [...document.querySelectorAll('#status-bar button:not([hidden])')].every(b=>b.getBoundingClientRect().height >= {44 if width == 375 else 20})"))
        if width == 375:
            test.assertEqual(evaluate("[...document.querySelectorAll('.sb-provider')].filter(b=>!b.hidden).length"), 1)
        if name.startswith('normal-bar-1280'):
            test.assertGreater(evaluate("document.querySelectorAll('.sb-provider:not([hidden])').length"), 1)
        if width == 375:
            test.assertEqual(evaluate("document.querySelector('#sb-overflow>span').getBoundingClientRect().height"), 18)
        if view == 'roster':
            test.assertEqual(evaluate("document.querySelectorAll('#sb-roster button').length"), 11 if name.startswith('signed-out') else 0)
            test.assertEqual(evaluate("getComputedStyle(document.querySelector('#sb-roster .sb-window+.sb-window'), '::before').content") if evaluate("!!document.querySelector('#sb-roster .sb-window+.sb-window')") else 'none', 'none')
        if not reference:
            return
        snapshot = rpc(dict(action='snapshot'))['snapshot']
        upstream = []
        for provider in snapshot['providers'][:10]:
            phase = provider['phase']
            status = {'loading': 'idle', 'off': 'unavailable', 'refreshing': 'fetching', 'ready': 'ok'}.get(phase, 'error')
            cached = phase not in ['loading', 'off', 'error_no_cache', 'not_signed_in']
            limits = [dict(usedPercent=w['percent_used'] or 0, windowMinutes=300 if w['label'] == '5h' else 10080, resetsAt=datetime.fromisoformat(w['reset_at_iso'].replace('Z', '+00:00')).timestamp()*1000) for w in provider['windows']] if cached else []
            upstream.append(dict(provider=provider['id'], status=status, planType=provider['plan'], updatedAt=(provider['updated_at'] or 0)*1000, error='not signed in' if phase == 'not_signed_in' else 'Refresh failed' if status == 'error' else None, session=limits[0] if limits else None, weekly=limits[1] if len(limits)>1 else None))
        enabled = [{'resources':'resource-usage','hosts':'ssh'}.get(k,k) for k, value in snapshot['visibility'].items() if value]
        query = urlencode(dict(id='status-bar/orquesta-reference:reference', theme=theme, view=view, state='fetching' if any(p['phase']=='refreshing' for p in snapshot['providers']) else 'ok', mode=snapshot['display_mode'], items=json.dumps(enabled), providers=json.dumps(upstream)))
        ref('open', f'http://127.0.0.1:5179/fixtures.html?{query}')
        ref('set', 'viewport', str(width), '900')
        ref('wait', '--fn', "document.documentElement.dataset.fixtureState === 'ready'")
        ref('screenshot', str(shots / f'{name}-orca.png'))
        gallery.append(f'<tr><th>{name}</th><td><img src="{name}.png"></td><td><img src="{name}-orca.png"></td></tr>')

    try:
        cases = [('loading','loading',0), ('normal','ready',42), ('warning','ready',60), ('urgent','ready',80), ('stale','stale',42), ('error','error_cached',42), ('no-cache','error_no_cache',0), ('off','off',0), ('signed-out','not_signed_in',0), ('refreshing','refreshing',42)]
        for case, phase, percent in cases:
            install(phase, percent)
            for width in [1280, 375]:
                browser('set', 'viewport', str(width), '900')
                browser('screenshot', '/tmp/orquesta-status-layout.png')
                for theme in ['light', 'dark']:
                    if phase == 'refreshing':
                        install(phase, percent)
                    if evaluate('document.documentElement.dataset.theme') != theme:
                        browser('click', '#theme')
                    palette = evaluate("[getComputedStyle(document.querySelector('#status-bar')).backgroundColor, ...['--status-warning','--destructive','--sb-off','--muted'].map(k=>getComputedStyle(document.querySelector('#status-bar')).getPropertyValue(k))]")
                    for color in palette[1:]:
                        test.assertGreaterEqual((max(luminance(color), luminance(palette[0]))+.05)/(min(luminance(color), luminance(palette[0]))+.05), 4.5)
                    for view in ['bar', 'roster'] + (['visibility'] if case == 'normal' else []):
                        if view == 'roster':
                            browser('click', '.sb-provider:not([hidden])')
                        if view == 'visibility':
                            browser('focus', '#sb-items')
                            browser('press', 'Shift+F10')
                        capture(f'{case}-{view}-{width}-{theme}', view, width, theme)
                        if case == 'normal' and view == 'roster' and width == 375 and theme == 'dark':
                            browser('scrollintoview', '#sb-roster [data-id="decision:local"]')
                            test.assertIn('42%', evaluate("document.querySelector('#sb-roster [data-id=\"decision:local\"]').textContent"))
                            browser('screenshot', str(shots / 'configured-entry-375-dark.png'))
                        if view != 'bar':
                            browser('press', 'Escape')
            if case == 'urgent':
                test.assertIn('urgent', evaluate("document.querySelector('#sb-announcement').textContent"))
        test.assertEqual(evaluate(draft), before)
        install('ready', 42)
        browser('click', '.sb-provider:not([hidden])')
        countdown = "document.querySelector('#sb-roster [data-reset]').textContent"
        prior = evaluate(countdown)
        evaluate("window.realNow=Date.now; Date.now=()=>window.realNow()+120000")
        browser('wait', '--fn', f"{countdown} !== {json.dumps(prior)}")
        evaluate('Date.now=window.realNow')
        evaluate("document.querySelector('#sb-refresh').click(); document.querySelector('#sb-refresh').click()")
        browser('wait', '--fn', "!document.querySelector('#sb-refresh').disabled")
        test.assertEqual(len(Path(str(fixture_file)+'.calls').read_text().splitlines()), 11)
        browser('press', 'Escape')
        browser('focus', '#sb-items')
        browser('press', 'ContextMenu')
        test.assertTrue(evaluate("document.querySelector('#sb-menu').matches(':popover-open')"))
        browser('click', '#sb-settings-open')
        test.assertEqual(evaluate("document.querySelectorAll('#sb-settings input[data-key]').length"), 15)
        for key in ['claude', 'decision:local', 'resources', 'ports', 'hosts']:
            browser('uncheck', f'#sb-settings input[data-key="{key}"]')
        browser('check', '#sb-settings input[data-mode="detailed"]')
        browser('press', 'Escape')
        browser('reload')
        persisted = rpc(dict(action='snapshot'))['snapshot']
        test.assertTrue(all(not persisted['visibility'][key] for key in ['claude','decision:local','resources','ports','hosts']))
        test.assertEqual(persisted['display_mode'], 'detailed')
        for kind in ['terminals', 'ports', 'hosts']:
            rpc(dict(action='visibility', key=kind, visible=True))
            rpc(dict(action='inventory', kind=kind, id='fixture', present=True))
            browser('wait', '--fn', f"document.querySelector('#sb-{kind}').ariaLabel.includes('1')")
            rpc(dict(action='inventory', kind=kind, id='fixture', present=False))
        for index, entry in enumerate(entries):
            rpc(dict(action='visibility', key=entry['id'], visible=index<3))
        browser('set', 'viewport', '1280', '900')
        browser('screenshot', '/tmp/orquesta-status-layout.png')
        browser('wait', '--fn', "document.querySelector('#sb-overflow').hidden")
        test.assertEqual(evaluate("[...document.querySelectorAll('.sb-provider:not([hidden])>.sb-meter')].map(m=>[m.offsetWidth,m.offsetHeight])"), [[48,6]]*3)
        for theme in ['light', 'dark']:
            if evaluate('document.documentElement.dataset.theme') != theme:
                browser('click', '#theme')
            capture(f'detailed-three-1280-{theme}', 'bar', 1280, theme)
        (shots/'comparison.html').write_text('<!doctype html><title>Status bar comparison</title><style>table{width:100%}img{width:100%}td{width:48%}</style><h1>Orquesta and isolated upstream usage components</h1><p>Fixture data, not live provider validation. Reference excludes the surface controller, inventory, and density behavior.</p><table>'+''.join(gallery)+'</table>')
    finally:
        if reference:
            ref('close')
            reference.terminate()
            reference.wait()
