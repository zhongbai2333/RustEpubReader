import re
CJK=re.compile('[\u3400-\u9fff]')
# Parse interpolated Swift/Kotlin strings without treating nested argument literals as outer text.
def balanced(s,i,end):
 depth=1;start=i
 while i<len(s):
  c=s[i]
  if c=='"':i=string(s,i,'kotlin' if end=='}' else 'swift')[0];continue
  if c==('{' if end=='}' else '('):depth+=1
  if c==end:
   depth-=1
   if depth==0:return i+1,s[start:i]
  i+=1
 raise ValueError('unbalanced interpolation')
def string(s,i,lang):
 i+=1;parts=[];args=[]
 while i<len(s):
  c=s[i]
  if c=='"':return i+1,''.join(parts),args
  if c=='\\':
   if lang=='swift' and i+1<len(s) and s[i+1]=='(':
    i,arg=balanced(s,i+2,')');parts.append('{'+str(len(args))+'}');args.append(arg);continue
   if i+1<len(s):
    escapes={'n':'\n','t':'\t','r':'\r','"':'"','\\':'\\'};parts.append(escapes.get(s[i+1],'\\'+s[i+1]));i+=2;continue
  if lang=='kotlin' and c=='$':
   if s[i+1:i+2]=='{':i,arg=balanced(s,i+2,'}')
   else:
    m=re.match(r'[A-Za-z_][\w]*',s[i+1:]);
    if not m:parts.append(c);i+=1;continue
    arg=m[0];i+=len(arg)+1
   parts.append('{'+str(len(args))+'}');args.append(arg);continue
  parts.append(c);i+=1
 raise ValueError('unterminated string')
def tokens(s,lang):
 i=0
 while i<len(s):
  if s.startswith('//',i):i=s.find('\n',i) if '\n' in s[i:] else len(s);continue
  if s.startswith('/*',i):
   depth=1;i+=2
   while i<len(s) and depth:
    if s.startswith('/*',i):depth+=1;i+=2
    elif s.startswith('*/',i):depth-=1;i+=2
    else:i+=1
   continue
  if s[i]=='"':
   a=i
   try:i,text,args=string(s,i,lang)
   except ValueError:i+=1;continue
   yield a,i,text,args
  else:i+=1
