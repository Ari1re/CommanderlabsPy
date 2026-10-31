#!/bin/bash
set -u
API=http://127.0.0.1:7878/api
post(){ curl -s -X POST $API/$1 -H 'content-type: application/json' -d "$2"; }
wait_job(){ for i in $(seq 1 400); do r=$(post job "{\"id\":$1}"); st=$(echo "$r" | python3 -c "import json,sys;print(json.load(sys.stdin)['data']['status'])"); [ "$st" != running ] && { echo "$r"; return; }; sleep 3; done; echo "$r"; }
job(){ post "$1" "$2" | python3 -c "import json,sys;d=json.load(sys.stdin);print(d['data']['job'] if d.get('ok') else -1)"; }
j=$(job update_cards '{}'); wait_job $j | head -c 300; echo
show(){ post deck "{\"name\":\"$1\"}" | python3 -c "
import json,sys; d=json.load(sys.stdin)['data']
print('FORMAT', d['format'], d['format_check'])
print('CARDS', ', '.join(e['name'] for e in d['entries']))
for n in d['plan']['notes']: print('NOTE', n[:900])
for s in d['plan']['sources'][:12]: print('SRC', s['source'], '|', s['title'], '|', s['url'])
print('SRC count', len(d['plan']['sources']))"; }
for spec in '{"format":"canlander","colors":"UR","theme":"Tempo","bracket":5}' '{"commander":"Murmuring Mystic","theme":"Flying","bracket":3,"format":"pdh"}'; do
  echo "=== GENERATE $spec"
  j=$(job generate "$spec"); r=$(wait_job $j); echo "$r" | python3 -c "import json,sys;d=json.load(sys.stdin)['data'];print(d['status'], d.get('error'), d.get('result')); print('\n'.join(d.get('log',[])[-25:]))"
  name=$(echo "$r" | python3 -c "import json,sys;d=json.load(sys.stdin)['data'];print((d.get('result') or {}).get('name',''))")
  [ -n "$name" ] && show "$name"
done
echo "=== LOG"; grep -rh "PDH commander\|PDHREC\|HIGHLANDER\|ARCHIDEKT DISCOVERY\|CORPUS COMPLETE" run/data/logs/ 2>/dev/null | tail -20
