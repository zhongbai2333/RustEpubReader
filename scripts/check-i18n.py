#!/usr/bin/env python3
"""Validate the shared catalogs and application references before native packaging."""
import argparse,collections,hashlib,json,re,sys
from pathlib import Path
from i18n_source_scan import tokens,string
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--ios-output');p.add_argument('--report');a=p.parse_args()
errors=[];catalogs={}
for code in ['en','zh_cn']:
 path=ROOT/'core/src/i18n'/f'{code}.json'
 pairs=json.loads(path.read_text(),object_pairs_hook=list)
 duplicates=[k for k,n in collections.Counter(k for k,v in pairs).items() if n>1]
 if duplicates:errors.append(f'{code}: duplicate keys {duplicates}')
 catalog=dict(pairs);catalogs[code]=catalog
 for k,v in pairs:
  if not isinstance(v,str) or not v.strip():errors.append(f'{code}: empty or non-string {k}')
en,zh=catalogs['en'],catalogs['zh_cn']
if en.keys()!=zh.keys():errors.append('Catalog keys differ: '+str(en.keys()^zh.keys()))
def signature(s):
 # Sequential legacy slots normalize to indexed slots for comparison.
 out=[];next_slot=0
 for m in re.finditer(r'\{\{|\}\}|\{([^{}]*)\}',s):
  if m[0] in ['{{','}}']:continue
  token=m[1]
  if token=='':token=str(next_slot);next_slot+=1
  out.append(token)
 printf=re.findall(r'%(?:\d+\$)?[\d.]*[dfsu@]',s)
 return sorted(out),sorted(printf)
for k in en.keys()&zh.keys():
 if signature(en[k])!=signature(zh[k]):errors.append(f'Placeholder mismatch {k}: {signature(en[k])} != {signature(zh[k])}')
 if re.search('[\u3400-\u9fff]',en[k]):errors.append(f'Chinese UI copy in English: {k}')
allow=json.loads((ROOT/'scripts/i18n-literal-allowlist.json').read_text());allowed={(r['path'],r['text']):r['reason'] for r in allow}
references=[];residual=[]
for folder,ext,lang in [('ios/Reader','swift','swift'),('android/app/src/main/java','kt','kotlin'),('desktop/src','rs','rust')]:
 for path in sorted((ROOT/folder).rglob('*.'+ext)):
  relative=str(path.relative_to(ROOT));source=path.read_text().split('#[cfg(test)]')[0]
  for match in re.finditer(r'(?:\bL|(?:\bi18n|\bI18n|self\.i18n)\.(?:t|tf|tf1|tf2|count))\(',source):
   start=match.end();at=start;depth=1
   while at<len(source):
    if source[at]=='"':
     try:at=string(source,at,lang)[0];continue
     except ValueError:break
    if source[at]=='(':depth+=1
    elif source[at]==')':
     depth-=1
     if depth==0:break
    elif source[at]==',' and depth==1:break
    at+=1
   for _,_,key,_ in tokens(source[start:at],lang):
    if not re.fullmatch(r'[a-z][a-z_]*\.[a-z0-9_.]+',key):continue
    references.append(key)
    if key not in en:errors.append(f'{relative}:{source.count(chr(10),0,match.start())+1}: missing {key}')
  # Conditional/dynamic label tables are reviewed in addition to direct call arguments.
  for start,end,text,args in tokens(source,lang):
   prefix=source[max(0,start-80):start]
   direct_ui=re.search(r'(?:\bText|\bon_hover_text|\bset_title|\bui\.(?:label|button|heading))\(\s*$|contentDescription\s*=\s*$',prefix)
   if direct_ui and re.search(r'[A-Za-z]',text) and not args and text not in {'EPUB','EPUB Reader','A','Aa','sk-...','https://api.example.com/translate','https://api.example.com/dictionary','192.168.1.2:14528'}:
    if not re.fullmatch(r'(?:[a-z_]+\.)+[a-z0-9_]+',text) and (relative,text) not in allowed:errors.append(f'{relative}:{source.count(chr(10),0,start)+1}: hardcoded UI label {text!r}')
   if re.search('[\u3400-\u9fff]',text):
    row={'path':relative,'text':text,'line':source.count('\n',0,start)+1};residual.append(row)
    if (relative,text) not in allowed:errors.append(f'{relative}:{row["line"]}: untranslated literal {text!r}')
if a.ios_output and not errors:
 output=Path(a.ios_output)
 for locale,code in [('en','en'),('zh-Hans','zh_cn')]:
  folder=output/(locale+'.lproj');folder.mkdir(parents=True,exist_ok=True)
  (folder/'InfoPlist.strings').write_text('"NSLocalNetworkUsageDescription" = '+json.dumps(catalogs[code]['permission.local_network'],ensure_ascii=False)+';\n')
report={'keys':len(en),'references':len(references),'errors':errors,'allowed_data_literals':residual,'catalog_sha256':{code:hashlib.sha256((ROOT/'core/src/i18n'/f'{code}.json').read_bytes()).hexdigest() for code in catalogs}}
if a.report:Path(a.report).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
for error in errors:print(error,file=sys.stderr)
print(f'i18n: {len(en)} shared keys, {len(references)} references, {len(errors)} errors')
sys.exit(bool(errors))
