#!/usr/bin/env python3
"""
Atlas MiniWars -> native data baker.   Usage:  python3 tools/bake.py path/to/miniwars.html . [--skip-assets]

 * extracts every embedded data: URI (flags, icons, fonts, audio, images) to assets/extracted/
 * rasterizes the built-in GeoJSON into compact grid maps (port of Rasterizer.rasterizeLand
   + GridOps.computeOutline / cleanupSmallIslands / computeInnerLine from the JS)
 * bakes country stats (pop / gdp / military / mobilizable manpower base)
 * bakes the (immutable) ocean-depth shading layer (port of Renderer._renderOceanDepth)
Needs: numpy, pillow, scipy, scikit-image
"""
import re, sys, os, json, math, base64, struct, zlib
import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage
from skimage import measure

SRC = sys.argv[1]; ROOT = sys.argv[2] if len(sys.argv) > 2 else '.'
html = open(SRC, encoding='utf-8', errors='replace').read()
os.makedirs(f'{ROOT}/assets/maps', exist_ok=True)

def extract_assets():
    out = f'{ROOT}/assets/extracted'
    for d in ('flags', 'fonts', 'audio', 'images', 'icons'):
        os.makedirs(f'{out}/{d}', exist_ok=True)
    fb = html.find('const EMBEDDED_FLAG_DATA')
    fe = html.find('});', fb) if fb >= 0 else -1
    manifest, n_img, n_aud = [], 0, 0
    for m in re.finditer(r'data:([a-z]+/[a-z0-9.+-]+);base64,([A-Za-z0-9+/=]+)', html):
        mime, b64 = m.group(1), m.group(2)
        if len(b64) < 200 and mime != 'image/svg+xml':
            continue
        pre = html[max(0, m.start() - 400):m.start()]
        key = re.search(r'([A-Za-z_][\w-]*)["\']?\s*[:=]\s*(?:url\()?["\'`]?$', pre[-90:])
        key = key.group(1) if key else None
        try:
            data = base64.b64decode(b64 + '=' * (-len(b64) % 4))
        except Exception:
            continue
        if mime == 'image/png':
            if fb >= 0 and fb < m.start() < fe and key:
                rel = f'flags/{key}.png'
            elif key and key.isupper() or (key and key.endswith('_DATA')):
                rel = f'icons/{key.lower()}.png'
            else:
                n_img += 1; rel = f'images/img_{n_img:03d}.png'
        elif mime == 'font/ttf':
            fam = re.findall(r"font-family:\s*'([^']+)'", pre)
            wt = re.findall(r'font-weight:\s*(\d+)', pre)
            rel = f"fonts/{(fam[-1] if fam else 'font').replace(' ', '')}-{wt[-1] if wt else '400'}.ttf"
        elif mime == 'audio/mpeg':
            n_aud += 1; rel = f'audio/{key or "sfx"}_{n_aud}.mp3'
        elif mime == 'image/svg+xml':
            n_img += 1; rel = f'images/vector_{n_img:03d}.svg'
        else:
            continue
        open(f'{out}/{rel}', 'wb').write(data)
        manifest.append({'file': rel, 'bytes': len(data), 'hint': key})
    json.dump(manifest, open(f'{out}/manifest.json', 'w'), indent=1)
    print('extracted', len(manifest), 'assets')

def json_after(marker):
    i = html.find(marker)
    j = html.find('{', i)
    obj, _ = json.JSONDecoder().raw_decode(html, j)
    return obj

AFRO = json_after('const BUILTIN_GEOJSON_AFRO_EURASIA')
EUROPE = re.search(r"europeCountries:\[(.*?)\]", html, re.S).group(1)
EUROPE = set(re.findall(r"'([^']+)'", EUROPE))
ci = html.find('countryColors:{')
COLORS = {n: (int(r), int(g), int(b)) for n, r, g, b in
          re.findall(r"'([^']+)':\[(\d+),(\d+),(\d+)\]", html[ci:html.find('countryIso:', ci)])}
CAPS = {n: (c, float(lo), float(la)) for n, c, lo, la in re.findall(
    r"'([^']+)':\{[^{}]*?capital:'([^']*)',\s*capLon:(-?[\d.]+),\s*capLat:(-?[\d.]+)", html)}

ii = html.find('countryIso:{')
ISO = dict(re.findall(r"'([^']+)':'([a-z]{2,3})'", html[ii:html.find('}', ii)])) if ii >= 0 else {}

si = html.find('countryStats:{')
STATS = {}
for n, body in re.findall(r"'([^']+)':\{([^{}]*\bpop:[^{}]*)\}", html[si:si + 60000]):
    g = lambda k: (re.search(r'\b' + k + r':(-?[\d.]+)', body) or [None, None])[1]
    if g('pop') is None or g('military') is None: continue
    STATS[n] = dict(pop=float(g('pop')), gdp=float(g('gdp') or 1), mil=float(g('military')), halved='halved:true' in body)

def stats_for(name, lab):
    if name in STATS: return STATS[name]
    pe, gd = lab[2], lab[3]
    pop = max(0.1, round(pe / 1e6, 1)) if pe else 5.0
    gdp = max(1.0, round(gd / 1e3, 1)) if gd else float(round(pop * 15))
    return dict(pop=pop, gdp=gdp, mil=max(2.0, float(round(pop * 2.5))), halved=False)

def hsl_to_rgb(h, s, l):
    c = (1 - abs(2 * l - 1)) * s; x = c * (1 - abs((h / 60) % 2 - 1)); m = l - c / 2
    if h < 60: r, g, b = c, x, 0
    elif h < 120: r, g, b = x, c, 0
    elif h < 180: r, g, b = 0, c, x
    elif h < 240: r, g, b = 0, x, c
    elif h < 300: r, g, b = x, 0, c
    else: r, g, b = c, 0, x
    f = lambda v: int(math.floor((v + m) * 255 + 0.5))
    return (f(r), f(g), f(b))

def color_for(name):
    if name in COLORS: return COLORS[name]
    h = 0
    for ch in name: h = (h * 31 + ord(ch)) & 0xFFFFFFFF
    return hsl_to_rgb(h % 360, 0.55, 0.45)

def rasterize(features, crop, W, H, allowed):
    SS = 2; SW, SH = W * SS, H * SS
    L0, L1, A0, A1 = crop
    to_ss = lambda lon, lat: ((lon - L0) / (L1 - L0) * SW, (A1 - lat) / (A1 - A0) * SH)
    land = np.zeros((H, W), np.uint8); cg = np.zeros((H, W), np.uint16)
    names, labels = [None], {}
    for f in features:
        g = f.get('geometry')
        if not g: continue
        p = f.get('properties') or {}
        name = p.get('ADMIN') or p.get('admin') or p.get('name') or p.get('NAME')
        if not name or (allowed is not None and name not in allowed): continue
        if g['type'] == 'Polygon': rings = list(g['coordinates'])
        elif g['type'] == 'MultiPolygon': rings = [r for poly in g['coordinates'] for r in poly]
        else: continue
        arrs = [np.asarray(r, dtype=np.float64)[:, :2] for r in rings if len(r) >= 3]
        if not arrs: continue
        lonmin = min(a[:, 0].min() for a in arrs); lonmax = max(a[:, 0].max() for a in arrs)
        latmin = min(a[:, 1].min() for a in arrs); latmax = max(a[:, 1].max() for a in arrs)
        if lonmax < L0 or lonmin > L1 or latmax < A0 or latmin > A1: continue
        pxmin, pymin = to_ss(max(lonmin, L0), min(latmax, A1))
        pxmax, pymax = to_ss(min(lonmax, L1), max(latmin, A0))
        sx0 = max(0, math.floor(pxmin)); sy0 = max(0, math.floor(pymin))
        sx1 = min(SW, math.ceil(pxmax)); sy1 = min(SH, math.ceil(pymax))
        bw, bh = sx1 - sx0, sy1 - sy0
        if bw <= 0 or bh <= 0: continue
        mask = np.zeros((bh, bw), bool)
        for a in arrs:
            px = (a[:, 0] - L0) / (L1 - L0) * SW - sx0 - 0.5
            py = (A1 - a[:, 1]) / (A1 - A0) * SH - sy0 - 0.5
            im = Image.new('1', (bw, bh), 0)
            ImageDraw.Draw(im).polygon(list(zip(px.tolist(), py.tolist())), fill=1)
            mask ^= np.asarray(im, dtype=bool)
        cx0, cy0 = sx0 // SS, sy0 // SS; cx1, cy1 = -(-sx1 // SS), -(-sy1 // SS)
        buf = np.zeros(((cy1 - cy0) * SS, (cx1 - cx0) * SS), bool)
        oy, ox = sy0 - cy0 * SS, sx0 - cx0 * SS
        buf[oy:oy + bh, ox:ox + bw] = mask
        lit = buf.reshape(cy1 - cy0, SS, cx1 - cx0, SS).any(axis=(1, 3))
        if not lit.any(): continue
        cid = len(names)
        land[cy0:cy1, cx0:cx1][lit] = 1
        sub = cg[cy0:cy1, cx0:cx1]; sub[lit] = cid
        names.append(name); labels[name] = (p.get('label_x'), p.get('label_y'), p.get('pop_est'), p.get('gdp_md'))
    return land, cg, names, labels

DIRS = [(dy, dx) for dy in (-1, 0, 1) for dx in (-1, 0, 1) if (dy, dx) != (0, 0)]
def shifted(a, dy, dx, fill=0):
    H, W = a.shape; out = np.full_like(a, fill)
    ys, yd = (slice(0, H - dy), slice(dy, H)) if dy >= 0 else (slice(-dy, H), slice(0, H + dy))
    xs, xd = (slice(0, W - dx), slice(dx, W)) if dx >= 0 else (slice(-dx, W), slice(0, W + dx))
    out[ys, xs] = a[yd, xd]
    return out

def outline_of(land, cg):
    H, W = land.shape
    inb = np.ones((H, W), np.uint8); b = np.zeros((H, W), bool)
    for dy, dx in DIRS:
        nin = shifted(inb, dy, dx, 0).astype(bool)
        nl = shifted(land, dy, dx, 0); nc = shifted(cg, dy, dx, 0)
        later = dy > 0 or (dy == 0 and dx > 0)
        b |= ~nin
        b |= nin & (nl == 0)
        if later: b |= nin & (nl == 1) & (nc != cg)
    return (land == 1) & b

def inner_of(land, outline):
    H, W = land.shape; near = np.zeros((H, W), bool)
    for dy, dx in DIRS: near |= shifted(outline.astype(np.uint8), dy, dx, 0).astype(bool)
    return (land == 1) & ~outline & near

def cleanup_islands(land, outline, small_max=7):
    lab = measure.label(land, connectivity=2)
    cnt = np.bincount(lab.ravel())
    small = (cnt <= small_max) & (np.arange(len(cnt)) > 0)
    outline |= small[lab] & (land == 1)

def _hash(x, y):
    M = 0xFFFFFFFF
    h = ((x.astype(np.int64) * 374761393) + (y.astype(np.int64) * 668265263)) & M
    h = ((h ^ (h >> 13)) * 1274126177) & M
    return ((h ^ (h >> 16)) & M) / 4294967295.0

def _noise(x, y, size):
    fx, fy = x / size, y / size
    ix, iy = np.floor(fx), np.floor(fy)
    u = (fx - ix) ** 2 * (3 - 2 * (fx - ix)); v = (fy - iy) ** 2 * (3 - 2 * (fy - iy))
    a, b = _hash(ix, iy), _hash(ix + 1, iy); c, d = _hash(ix, iy + 1), _hash(ix + 1, iy + 1)
    return (a + (b - a) * u) * (1 - v) + (c + (d - c) * u) * v

def ocean_layer(land, reach=34):
    H, W = land.shape
    dist = ndimage.distance_transform_cdt(land == 0, metric='taxicab').astype(np.float64)
    dist[dist > reach] = 255
    y, x = np.mgrid[0:H, 0:W].astype(np.float64)
    relief = (_noise(x, y, 79) * 2 - 1) * .72 + (_noise(x + 137, y - 71, 27) * 2 - 1) * .28
    shelf = np.clip((reach - dist + relief * 7) / reach, 0, 1) ** 1.4
    col = (58, 111, 143)
    rgba = np.zeros((H, W, 4), np.uint8)
    cl = lambda v: np.clip(np.floor(v + .5), 0, 255).astype(np.uint8)
    rgba[..., 0] = cl(col[0] - 27 + 76 * shelf + relief * 9)
    rgba[..., 1] = cl(col[1] - 27 + 76 * shelf + relief * 10)
    rgba[..., 2] = cl(col[2] - 32 + 61 * shelf + relief * 9)
    rgba[..., 3] = cl(88 + 38 * shelf + relief * 24)
    rgba[land == 1] = 0
    return rgba

def write_map(map_id, crop, W, H, allowed):
    print('baking', map_id, W, H)
    land, cg, names, labels = rasterize(AFRO['features'], crop, W, H, allowed)
    outline = outline_of(land, cg); cleanup_islands(land, outline); inner = inner_of(land, outline)
    L0, L1, A0, A1 = crop
    caps = []
    for cid, n in enumerate(names):
        if cid == 0: continue
        lon, lat = (CAPS[n][1], CAPS[n][2]) if n in CAPS else labels[n][:2]
        if lon is None or lat is None: continue
        cx = math.floor((lon - L0) / (L1 - L0) * W); cy = math.floor((A1 - lat) / (A1 - A0) * H)
        if 0 <= cx < W and 0 <= cy < H: caps.append((cid, cx, cy))
    st = {n: stats_for(n, labels[n]) for n in names[1:]}
    ratios = sorted(s['mil'] / max(0.05, s['pop']) for s in st.values())
    median = ratios[len(ratios) // 2] or 1.0
    def mobil(s):
        rel = (s['mil'] / max(0.05, s['pop'])) / median if median > 0 else 1.0
        mod = min(1.3, max(0.75, max(0.0001, rel) ** 0.4))
        return s['pop'] * 12000 * mod * (0.5 if s['halved'] else 1.0)
    blob = bytearray(b'ATLM')
    blob += struct.pack('<HHHHH', 3, W, H, len(names) - 1, len(caps))
    blob += struct.pack('<4f', L0, L1, A0, A1)
    for n in names[1:]:
        nb = n.encode(); r, g, b = color_for(n)
        blob += struct.pack('<B', len(nb)) + nb + bytes((r, g, b)) + struct.pack('<4f', st[n]['pop'], st[n]['gdp'], st[n]['mil'], mobil(st[n])) + struct.pack('<B', len(ISO.get(n, ''))) + ISO.get(n, '').encode()
    for c in caps: blob += struct.pack('<HHH', *c)
    blob += land.tobytes() + cg.astype('<u2').tobytes() + outline.astype(np.uint8).tobytes() + inner.astype(np.uint8).tobytes()
    packed = zlib.compress(bytes(blob), 9)
    open(f'{ROOT}/assets/maps/{map_id}.atlasmap.z', 'wb').write(packed)
    Image.fromarray(ocean_layer(land), 'RGBA').save(f'{ROOT}/assets/maps/{map_id}_ocean.png', optimize=True)
    print(f'  land={int(land.sum())} countries={len(names)-1} capitals={len(caps)} packed={len(packed)//1024}KB')

if __name__ == '__main__':
    if '--skip-assets' not in sys.argv: extract_assets()
    write_map('europe-2025', (-25, 45, 33, 72), 900, 710, EUROPE)
    write_map('afro-eurasia', (-28, 192, -36, 78), 1200, 680, None)
