import json, re, time, urllib.request, urllib.parse
UA={'User-Agent':'Mozilla/5.0 (X11; Linux x86_64) CommanderLab-probe','Accept':'application/json, text/html'}
def get(u):
    try:
        r=urllib.request.urlopen(urllib.request.Request(u,headers=UA),timeout=30); return r.status, r.read().decode('utf-8','replace')
    except Exception as e: return str(e)[:80],''
print('=== PDHREC home')
s,b=get('https://pdhrec.com/')
print(s, len(b)); print('next data:', '__NEXT_DATA__' in b, 'nuxt' in b.lower(), 'api' , re.findall(r'(?:src|href)="([^"]+\.js[^"]*)"',b)[:6])
print('links:', sorted(set(re.findall(r'href="(/[^"#?]{2,80})"',b)))[:40])
print(re.sub(r'\s+',' ',re.sub(r'<[^>]+>',' ',b))[:1500])
for u in ['https://pdhrec.com/commander/murmuring-mystic','https://pdhrec.com/commanders/murmuring-mystic','https://pdhrec.com/api/commanders','https://pdhrec.com/commander/murmuring-mystic/','https://pdhrec.com/sitemap.xml','https://pdhrec.com/robots.txt']:
    s,b=get(u); print('---',u,s,len(b)); print(re.sub(r'\s+',' ',re.sub(r'<[^>]+>',' ',b))[:700])
    m=re.search(r'<script id="__NEXT_DATA__"[^>]*>(.*?)</script>',b,re.S)
    if m: print('NEXT_DATA keys', list(json.loads(m.group(1)).get('props',{}).get('pageProps',{}).keys())[:20], m.group(1)[:800])
print('=== MOXFIELD pauperEdh commander filter')
s,b=get('https://api2.moxfield.com/v2/decks/search?pageNumber=1&pageSize=5&sortType=Views&sortDirection=Descending&fmt=pauperEdh&commander=Murmuring%20Mystic')
try:
    d=json.loads(b); print(s, d.get('totalResults'), [ (x.get('name'), x.get('format')) for x in d.get('data',[])[:5]]); print(json.dumps(d.get('data',[{}])[0])[:900])
except Exception: print(s,b[:200])
for extra in ['&colorIdentity=UR','&colors=UR','&filter=UR','&q=izzet']:
    s,b=get('https://api2.moxfield.com/v2/decks/search?pageNumber=1&pageSize=5&sortType=Views&sortDirection=Descending&fmt=highlanderCanadian'+extra)
    try: d=json.loads(b); print('CH',extra, s, d.get('totalResults'), [ (x.get('name'), x.get('colorIdentity') or x.get('colors')) for x in d.get('data',[])[:4]])
    except Exception: print('CH',extra,s,b[:120])
print('=== ARCHIDEKT 11-31 slow')
for n in range(11,32):
    time.sleep(4)
    s,b=get(f'https://archidekt.com/api/decks/v3/?deckFormat={n}&orderBy=-viewCount&pageSize=2')
    try: d=json.loads(b); print(n, s, d.get('count'), [x.get('name') for x in d.get('results',[])[:2]])
    except Exception: print(n, s)
print('=== MTGTOP8 CHL')
s,b=get('https://www.mtgtop8.com/format?f=CHL')
print('archetypes:', re.findall(r'archetype\?a=(\d+)&meta=(\d+)&f=CHL[^>]*>([^<]{2,40})</a>',b)[:40])
print('recent events:', re.findall(r'event\?e=(\d+)&f=CHL[^>]*>([^<]{3,60})<',b)[:10])
s,b=get('https://www.mtgtop8.com/archetype?a=480&meta=105&f=CHL')
print('RDW decks:', re.findall(r'event\?e=(\d+)&d=(\d+)&f=CHL[^>]*>([^<]{2,50})<',b)[:10])
s,b=get('https://www.mtgtop8.com/mtgo?d=531772'); print('export tail:', b[-300:])
