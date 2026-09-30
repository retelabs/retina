# Generates the Retina mark (an R drawn as a five-line music staff wound into a
# spiral galaxy, with a constellation of note-stars rising to a star). Run:
#   python3 UI/assets/logos/generate.py   (from this directory: writes the SVGs here)
# PNG exports: magick -background none retina-mark-dark.svg -resize 1024x1024 retina-mark-dark-1024.png
import math,sys
NAVY="#0B1026"; GOLD="#F2C14E"; VIOLET="#7B6CF6"
def f(v): return f"{v:.2f}"
def bez(p0,p1,p2,p3,n=60):
    out=[]
    for i in range(n+1):
        t=i/n; u=1-t
        out.append((u**3*p0[0]+3*u*u*t*p1[0]+3*u*t*t*p2[0]+t**3*p3[0],
                    u**3*p0[1]+3*u*u*t*p1[1]+3*u*t*t*p2[1]+t**3*p3[1]))
    return out
def chain(segs):
    pts=[]
    for s in segs:
        p=bez(*s); pts+= p if not pts else p[1:]
    return pts
def normals(pts):
    ns=[]
    for i in range(len(pts)):
        a=pts[max(i-1,0)]; b=pts[min(i+1,len(pts)-1)]
        dx,dy=b[0]-a[0],b[1]-a[1]; L=math.hypot(dx,dy) or 1
        ns.append((-dy/L,dx/L))
    return ns
def offset(pts,k,spacing):
    ns=normals(pts); out=[]
    n=len(pts)
    for i,(p,nv) in enumerate(zip(pts,ns)):
        s=spacing(i/(n-1))
        out.append((p[0]+nv[0]*k*s,p[1]+nv[1]*k*s))
    return out
def path(pts): return "M"+" L".join(f"{f(x)},{f(y)}" for x,y in pts)
def arclen_point(pts,frac):
    L=[0]
    for i in range(1,len(pts)): L.append(L[-1]+math.dist(pts[i-1],pts[i]))
    target=frac*L[-1]
    for i in range(1,len(pts)):
        if L[i]>=target:
            t=(target-L[i-1])/((L[i]-L[i-1]) or 1)
            return (pts[i-1][0]+t*(pts[i][0]-pts[i-1][0]),pts[i-1][1]+t*(pts[i][1]-pts[i-1][1]))
    return pts[-1]
def sub(pts,a,b):
    n=len(pts); return pts[int(a*(n-1)):int(b*(n-1))+1]

SP=8.5
def smooth(t,a):
    if t<=a: return 1
    x=(t-a)/(1-a); return max(0.04,1-x*x*(3-2*x))
main=chain([((-90,180),(-90,120),(-90,80),(-90,30)),
            ((-90,30),(-91,-70),(-50,-128),(20,-130)),
            ((20,-130),(78,-131),(122,-134),(150,-158))])
def sp_main(t):
    return SP*smooth(t,0.62)
cx,cy=0,-22
R_END=66; B=0.2; TH=2.1*2*math.pi
r0=R_END/math.exp(B*TH)
spi=[]
for i in range(260):
    th=TH*i/259
    r=r0*math.exp(B*th); a=math.pi/2+(TH-th)
    spi.append((cx+r*math.cos(a),cy+r*math.sin(a)))
ex,ey=spi[-1]
leg=bez((ex,ey),(ex+45,ey+9),(ex+68,ey+40),(ex+92,ey+76))[1:]+bez((ex+92,ey+76),(ex+116,ey+112),(ex+140,ey+126),(ex+182,ey+122))[1:]
sp=spi+leg
NS=len(spi); NT=len(sp)
def sp_spiral(t):
    i=t*(NT-1)
    core=min(1,(i/NS)/0.55)**0.9 if i<NS else 1
    tail=smooth(t,0.86)
    return SP*max(0.06,core)*tail
leg=None
K=[-2,-1,0,1,2]
def mark(line=VIOLET,star=GOLD,bg=NAVY,with_bg=True,glow=True,small=False):
    g=[]
    if with_bg: g.append(f'<rect x="-176" y="-231" width="440" height="440" fill="{bg}"/>')
    for base,spf,w in ((main,sp_main,3.0),(sp,sp_spiral,2.8)):
        for k in ((-1.6,0,1.6) if small else K):
            g.append(f'<path d="{path(offset(base,k,spf))}" fill="none" stroke="{line}" stroke-width="{11 if small else w}" stroke-linecap="round" stroke-linejoin="round"/>')
    # constellation: along a lane between lines
    lane=lambda base,spf,a,b: sub(offset(base,-0.5,spf),a,b)
    if small:
        sx,sy=main[-1]
        g.append(f'<circle cx="{cx}" cy="{cy}" r="16" fill="{star}"/>')
        g.append(f'<circle cx="{f(sx)}" cy="{f(sy)}" r="24" fill="{star}"/>')
        return "".join(g)
    Lm=offset(main,-0.5,sp_main); Ls=offset(sp,-0.5,sp_spiral)
    for d in (sub(Lm,0.2,0.975), sub(Ls,0.42,0.9)):
        g.append(f'<path d="{path(d)}" fill="none" stroke="{star}" stroke-width="2" stroke-dasharray="6 5" stroke-linecap="round"/>')
    dots=[(arclen_point(Lm,fr),6.5) for fr in (0.2,0.4,0.56,0.72,0.86)]
    dots+=[(arclen_point(Ls,fr),6) for fr in (0.42,0.6,0.76,0.9)]
    for (x,y),r in dots: g.append(f'<circle cx="{f(x)}" cy="{f(y)}" r="{r}" fill="{star}"/>')
    # core
    g.append(f'<circle cx="{cx}" cy="{cy}" r="9" fill="{star}"/>')
    sx,sy=main[-1]
    if glow: g.append(f'<circle cx="{f(sx)}" cy="{f(sy)}" r="16" fill="{star}" opacity="0.22"/>')
    for (dx,dy,L,w) in ((1,0,46,2.2),(-1,0,40,2.2),(0,-1,44,2.2),(0,1,26,2.2)):
        px,py=-dy,dx
        g.append(f'<path d="M{f(sx+px*w)},{f(sy+py*w)} L{f(sx+dx*L)},{f(sy+dy*L)} L{f(sx-px*w)},{f(sy-py*w)} Z" fill="{star}"/>')
    g.append(f'<circle cx="{f(sx)}" cy="{f(sy)}" r="9" fill="{star}"/>')
    return "".join(g)
def svg(inner,size=1024): return f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="-176 -231 440 440">{inner}</svg>'
open("retina-mark-dark.svg","w").write(svg(mark()))
open("retina-mark-light.svg","w").write(svg(mark(line="#4B3FC4",star="#D99A0B",bg="#F6F3EA",glow=False)))
open("retina-mark-transparent.svg","w").write(svg(mark(with_bg=False)))

open("retina-mark-small-dark.svg","w").write(svg(mark(small=True)))
open("retina-mark-small-light.svg","w").write(svg(mark(line="#4B3FC4",star="#D99A0B",bg="#F6F3EA",glow=False,small=True)))
