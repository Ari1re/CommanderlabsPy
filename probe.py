import json, re, time, urllib.request
UA={'User-Agent':'Mozilla/5.0 (X11; Linux x86_64) CommanderLab-probe','Accept':'application/json, text/html'}
def get(u):
    try:
        r=urllib.request.urlopen(urllib.request.Request(u,headers=UA),timeout=30); return r.status, r.read().decode('utf-8','replace')
    except Exception as e: return str(e)[:80],''
s,b=get('https://pdhrec.com/commander/murmuring-mystic/')
i=b.find('Airborne Aid'); print('=== PDHREC card html'); print(b[i-1500:i+700])
i=b.find('Types'); print('=== tabs'); print(b[i-600:i+600])
print('=== section headers', re.findall(r'<h[1-4][^>]*>([^<]{2,60})</h[1-4]>', b)[:40])
print('count of "of 193 decks":', b.count('of 193 decks'))
s,b2=get('https://pdhrec.com/commander/tatyova-benthic-druid/'); print('tatyova', s, b2.count('% of '))
s,b3=get('https://pdhrec.com/commander/breeches-brazen-plunderer-malcolm-keen-eyed-navigator/'); print('partner', s, b3.count('% of '))
s,b4=get('https://pdhrec.com/staples'); i=b4.find('In '); print('=== staples', s); print(re.sub(r'\s+',' ',re.sub(r'<[^>]+>',' ',b4))[:600])
time.sleep(3)
s,b=get('https://archidekt.com/api/decks/v3/?deckFormat=17&commanderName=Murmuring%20Mystic&orderBy=-viewCount&pageSize=5')
try: d=json.loads(b); print('=== archidekt pdh mystic', s, d.get('count'), [x.get('name') for x in d.get('results',[])])
except Exception: print('archidekt', s, b[:200])
s,b=get('https://www.mtgtop8.com/archetype?a=564&meta=105&f=CHL')
i=b.find('event?e='); print('=== mtgtop8 archetype rows'); print(b[i-800:i+900])
