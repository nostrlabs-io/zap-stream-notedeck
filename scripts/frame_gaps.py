import sys,re,datetime
ts=[]
for l in open(sys.argv[1],errors='ignore'):
    if 'Received HardwareBuffer frame' in l:
        m=re.match(r'\d+-\d+ (\d+):(\d+):([\d.]+)',l)
        ts.append(int(m[1])*3600+int(m[2])*60+float(m[3]))
d=sorted((b-a)*1000 for a,b in zip(ts,ts[1:]) if b-a<2)
raw=[(b-a)*1000 for a,b in zip(ts,ts[1:]) if b-a<2]
n=len(raw)
print("frames",len(ts),"p50 %.0f p95 %.0f p99 %.0f"%(d[n//2],d[int(n*.95)],d[int(n*.99)]),"share>70ms %.3f"%(sum(x>70 for x in raw)/n),"share>120ms %.3f"%(sum(x>120 for x in raw)/n))
