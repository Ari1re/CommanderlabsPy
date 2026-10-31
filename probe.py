import json, re, urllib.request, urllib.parse
UA={'User-Agent':'Mozilla/5.0 (X11; Linux x86_64) CommanderLab-probe','Accept':'application/json, text/html'}
def get(u, n=400):
    try:
        r=urllib.request.urlopen(urllib.request.Request(u,headers=UA),timeout=30); b=r.read().decode('utf-8','replace'); return r.status,b
    except Exception as e: return str(e)[:80],''
print('=== ARCHIDEKT formats')
for n in range(1,32):
    s,b=get(f'https://archidekt.com/api/decks/v3/?deckFormat={n}&orderBy=-viewCount&pageSize=3')
    try:
        d=json.loads(b); res=d.get('results',[])
        print(n, s, d.get('count'), [ (x.get('name'), x.get('deckFormat')) for x in res[:2]])
    except Exception: print(n, s, b[:80])
s,b=get('https://archidekt.com/api/decks/v3/?deckFormat=17&commanderName=Murmuring%20Mystic&orderBy=-viewCount&pageSize=5')
print('archidekt pdh commander search', s, b[:600])
print('=== MOXFIELD fmt')
for f in ['paupercommander','pauperEdh','pauperedh','pdh','highlanderCanadian','canadianhighlander','canlander','highlander','commander']:
    s,b=get(f'https://api2.moxfield.com/v2/decks/search?pageNumber=1&pageSize=3&sortType=Views&sortDirection=Descending&fmt={f}')
    try: d=json.loads(b); print(f, s, d.get('totalResults'), [x.get('name') for x in d.get('data',[])[:2]], [x.get('format') for x in d.get('data',[])[:2]])
    except Exception: print(f, s, b[:100])
print('=== MTGTOP8')
for f in ['CHL','PDH','pEDH','PAU','EDH','cEDH']:
    s,b=get(f'https://www.mtgtop8.com/format?f={f}')
    title=re.search(r'<title>(.*?)</title>',b,re.S)
    arch=re.findall(r'archetype\?a=(\d+)[^"]*"[^>]*>([^<]{2,40})<',b)[:8]
    print(f, s, title.group(1).strip()[:80] if title else '', arch)
s,b=get('https://www.mtgtop8.com/archetype?a=482&meta=105&f=CHL')
decks=re.findall(r'event\?e=(\d+)&d=(\d+)&f=CHL',b)[:5]
print('CHL rocks decks', decks)
if decks:
    s,b=get(f'https://www.mtgtop8.com/mtgo?d={decks[0][1]}')
    print('mtgo export', s, b[:400])
s,b=get('https://www.mtgtop8.com/format?f=CHL&meta=105')
print('meta links', re.findall(r'meta=(\d+)&f=CHL[^>]*>([^<]{3,40})<',b)[:12])
print('=== MTGDECKS')
for u in ['https://mtgdecks.net/Canadian-Highlander','https://mtgdecks.net/Pauper-EDH','https://mtgdecks.net/Pauper-Commander','https://mtgdecks.net/PDH']:
    s,b=get(u); t=re.search(r'<title>(.*?)</title>',b,re.S); print(u, s, t.group(1).strip()[:80] if t else '')
print('=== PDHREC / others')
for u in ['https://pdhrec.com/','https://www.pdhrec.com/','https://pdhhomebase.com/','https://json.edhrec.com/pages/commanders/murmuring-mystic.json']:
    s,b=get(u); t=re.search(r'<title>(.*?)</title>',b,re.S); print(u, s, (t.group(1).strip()[:80] if t else b[:120]))
