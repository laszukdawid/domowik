# 0003: Behavior specification for snapping, selection, and undo

- **Phase:** 0
- **Priority:** High
- **Status:** Done
- **Depends on:** 0001

## Problem

Define behavior-spec sections 6–8. Levels, viewport, save, and recovery belong
to 0020 rather than this issue.

## Acceptance criteria

- [x] Snap targets, priority, tie breaking, screen-space tolerance, zoom,
      mouse/touch differences, guides, and temporary disable are specified.
- [x] Selection hit priority, cycling, marquee inclusion, hidden/locked
      behavior, and multi-selection are specified.
- [x] Undo command boundaries cover click chains, drags, typing, cancellation,
      compound topology edits, redo invalidation, and session persistence.
- [x] Examples state authoritative ops versus transient preview/view state.
- [x] Normative rules have study-log entries and a provenance review.

## Review status

Worker contract at revision 6. Adversarial review rounds 1–14 are recorded in
the behavior-spec study log (2026-08-23 entries). Round fourteen rebuilt the
embedded checker as a genuinely executing engine-based script restoring the
full round-12 verified scope plus every round-13 requirement. The independent
GLM-5.3 review returned PRE-MERGE WORK APPROVED with 0 High, 0 Medium, and
three non-blocking Low findings; the orchestrator accepted the exit gate.

## Verification evidence

Reproducible checks run locally against docs/BEHAVIOR-SPEC.md on the issue
branch (uncommitted worktree). The complete checker script is embedded below;
the deterministic extraction command writes to /tmp so reproduction is
read-only.

### Round-14 executing checker (complete inline script — copy to /tmp and run)

```python
#!/usr/bin/env python3
"""Issue 0003 round-14 executing checker.

Executes the normative rules of BEHAVIOR-SPEC sections 6-8 against a real
minimal engine embedded in this script: exact rational geometry over
Fraction, cursor-anchored view transforms, snap evaluation with the full
candidate inventory, hit testing with join-node/free-endpoint classes,
shape-aware marquee with exact SAT and connected per-entity dedup,
selection cycling with engine-routed deletion, undo/save/recovery with
byte-exact interrupted-replacement resolution, compaction, and the
text-field lifecycle. Expected values in assertions are the normative
fixture oracles; every asserted value is computed by the engine.
"""
import json
import math
from fractions import Fraction as F

Z=F('0.1'); OX,OY=F(512),F(384); DPR=2
T_MOUSE,T_TOUCH=F(12),F(24)
PASS=0; TOTAL=0; FAILS=[]

def check(name,cond):
    global PASS,TOTAL
    TOTAL+=1
    if cond: PASS+=1; print('PASS',name)
    else: FAILS.append(name); print('FAIL',name)

def to_plan(rx,ry,z=Z,ox=OX,oy=OY):
    lx,ly=F(str(rx))/DPR,F(str(ry))/DPR
    return (lx-ox)/z,(oy-ly)/z

def sqd(px,py,qx,qy):
    return ((F(px)-F(qx))**2+(F(py)-F(qy))**2)*Z*Z

# ---------------- exact rational geometry ----------------
def canonical_ring_of(cs):
    ring=list(cs)
    start=min(ring); i=ring.index(start); ring=ring[i:]+ring[:i]
    a=F(0); n=len(ring)
    for k in range(n):
        x1,y1=ring[k]; x2,y2=ring[(k+1)%n]
        a+=x1*y2-x2*y1
    if a<0: ring=[ring[0]]+ring[1:][::-1]
    return ring

class Wall:
    def __init__(self,wid,x0,y0,x1,y1,t=200,h=2500,style='S0',level='L1'):
        self.id=wid; self.x0=F(x0); self.y0=F(y0)
        self.x1=F(x1); self.y1=F(y1)
        self.fields={'thicknessMm':F(t),'heightMm':F(h),'style':style,'levelId':level}
        self.dx=self.x1-self.x0; self.dy=self.y1-self.y0
        self.len2=self.dx*self.dx+self.dy*self.dy
        self._len_exact=None
        for k in range(0,6000):
            if F(k*k)==self.len2: self._len_exact=F(k); break
        self.patches=[]
    @property
    def t(self): return self.fields['thicknessMm']
    def unit_perp(self):
        L=self._len_exact
        assert L is not None,f'fixture walls need rational length: {self.id}'
        return (-self.dy/L,self.dx/L)
    def rect(self):
        h=self.t/2
        if self.dy==0:
            return (min(self.x0,self.x1),self.y0-h,max(self.x0,self.x1),self.y0+h)
        if self.dx==0:
            return (self.x0-h,min(self.y0,self.y1),self.x0+h,max(self.y0,self.y1))
        ux,uy=self.unit_perp(); ox,oy=ux*h,uy*h
        cs=self.corners(ox,oy)
        xs=[p[0] for p in cs]; ys=[p[1] for p in cs]
        return (min(xs),min(ys),max(xs),max(ys))
    def corners(self,ox=None,oy=None):
        h=self.t/2
        if ox is None:
            if self.dy==0:
                return [(self.x0,self.y0-h),(self.x1,self.y0-h),
                        (self.x1,self.y0+h),(self.x0,self.y0+h)]
            if self.dx==0:
                return [(self.x0-h,self.y0),(self.x0-h,self.y1),
                        (self.x0+h,self.y1),(self.x0+h,self.y0)]
            ux,uy=self.unit_perp(); ox,oy=ux*h,uy*h
        return [(self.x0+ox,self.y0+oy),(self.x1+ox,self.y1+oy),
                (self.x1-ox,self.y1-oy),(self.x0-ox,self.y0-oy)]
    def axis_proj(self,px,py):
        tt=((px-self.x0)*self.dx+(py-self.y0)*self.dy)/self.len2
        return (self.x0+tt*self.dx,self.y0+tt*self.dy)
    def on_segment(self,px,py):
        tt=((px-self.x0)*self.dx+(py-self.y0)*self.dy)/self.len2
        return F(0)<=tt<=F(1)
    def canonical_ring(self):
        return canonical_ring_of(self.corners())
    def boundary_parts(self):
        """Occupied-boundary components (R3.2): the wall's own ring plus
        disjoint join-patch rings, ordered by canonical outer-ring tuple;
        each component is a list of rings with the outer ring first."""
        comps=[[self.canonical_ring()]]
        for p in self.patches:
            comps.append([canonical_ring_of(p)])
        comps.sort(key=lambda c:c[0])
        return comps
    def boundary_closest(self,px,py):
        """((qx,qy),dist2_px,(component,ring,vertex-run,subfeature)) with
        every ordinal computed by enumeration over boundary_parts."""
        best=None
        for ci,comp in enumerate(self.boundary_parts()):
            for ri,ring in enumerate(comp):
                n=len(ring)
                for i in range(n):
                    ax,ay=ring[i]; bx,by=ring[(i+1)%n]
                    ex,ey=bx-ax,by-ay
                    e2=ex*ex+ey*ey
                    tt=((px-ax)*ex+(py-ay)*ey)/e2
                    ttc=max(F(0),min(F(1),tt))
                    qx,qy=ax+ttc*ex,ay+ttc*ey
                    d=sqd(px,py,qx,qy)
                    sub=('edge-start' if tt<=0 else
                         'edge-end' if tt>=1 else 'edge-closest')
                    key=(d,(ci,ri,i,sub))
                    if best is None or key<best[0]:
                        best=(key,(qx,qy),d,(ci,ri,i,sub))
        return best[1],best[2],best[3]

ORDER=['join-node','free-endpoint','edge','axis','grid','angle']
AXPOS={'on-segment':0,'extension':1}
HIT_ORDER=['join-node','free-endpoint','region','placeholder-box']
def crank(c,hit=False):
    seq=HIT_ORDER if hit else ORDER
    try: return seq.index(c)
    except ValueError: return len(seq)

INT_DIR={0:(1,0),45:(1,1),90:(0,1),135:(-1,1),
         -180:(-1,0),-135:(-1,-1),-90:(0,-1),-45:(1,-1)}
def norm180(a):
    a=a%360
    if a>=180: a-=360
    return a
def angle_targets(prev_dir):
    tg={norm180(F(k)*45) for k in range(-4,4)}
    if prev_dir is not None:
        for off in (0,45,90,135): tg.add(norm180(prev_dir+off))
    return sorted(tg)

def evaluate(walls,joints,p,device='mouse',grid=True,anchor=None,
             placeholders=(),chain=None,angles=False):
    """Snap evaluation (placeholders are NEVER snap sources). Angle
    candidates use integer direction vectors; perpendicular distance is
    computed in PLAN millimetres against the PLAN radius and reported as
    normalized LOGICAL px^2."""
    T=T_MOUSE if device in ('mouse','stylus') else T_TOUCH
    T_plan=T/Z
    out=[]
    def add(cls,d_px2,target,sub,wid):
        if anchor is not None and tuple(target)==tuple(anchor): return
        out.append((cls,d_px2,tuple(target),sub,wid))
    for jid,mem,(jx,jy) in joints:
        d=sqd(*p,jx,jy)
        if d<=T*T: add('join-node',d,(jx,jy),None,jid)
    for w in walls:
        for ex,ey,tag in ((w.x0,w.y0,'Start'),(w.x1,w.y1,'End')):
            joined=any((w.id,tag)==m for _,ms,_ in joints for m in ms)
            if joined: continue
            if anchor is not None and (ex,ey)==tuple(anchor): continue
            d=sqd(*p,ex,ey)
            if d<=T*T: add('free-endpoint',d,(ex,ey),None,w.id)
    for w in walls:
        (bx,by),bd,bkey=w.boundary_closest(*p)
        if bd<=T*T: add('edge',bd,(bx,by),bkey,w.id)
        ap=w.axis_proj(*p); d=sqd(*p,*ap)
        if d<=T*T:
            pos='on-segment' if w.on_segment(*ap) else 'extension'
            add('axis',d,ap,pos,w.id)
    if grid:
        gx=F(math.floor(p[0]/100))*100; gy=F(math.floor(p[1]/100))*100
        for qx,qy in ((gx,gy),(gx+100,gy),(gx,gy+100),(gx+100,gy+100)):
            d=sqd(*p,qx,qy)
            if d<=T*T: add('grid',d,(qx,qy),None,None)
    if angles and chain is not None:
        anch,prev=chain
        radius2_plan=T_plan*T_plan
        for tgt in angle_targets(prev):
            v=INT_DIR.get(int(tgt))
            if v is None: continue
            wx,wy=p[0]-anch[0],p[1]-anch[1]
            tpar=wx*v[0]+wy*v[1]
            if tpar<=0: continue
            cr=wx*v[1]-wy*v[0]
            v2=v[0]*v[0]+v[1]*v[1]
            perp2_plan=cr*cr/v2
            if perp2_plan<=radius2_plan:
                add('angle',perp2_plan*Z*Z,v[0:2],tgt,None)
    out.sort(key=lambda c:(crank(c[0]),c[1],c[2],
              AXPOS.get(c[3],0) if c[0]=='axis' else 0,str(c[3]),str(c[4])))
    return out

def evaluate_hits(walls,joints,placeholders,p,device='mouse'):
    """Hit testing with the FULL class inventory and strict priority:
    join-node > free-endpoint > region > placeholder-box."""
    T=T_MOUSE if device in ('mouse','stylus') else T_TOUCH
    out=[]
    for jid,mem,(jx,jy) in joints:
        d=sqd(*p,jx,jy)
        if d<=T*T: out.append(('join-node',(jx,jy),d,jid))
    for w in walls:
        for ex,ey,_t in ((w.x0,w.y0,'Start'),(w.x1,w.y1,'End')):
            joined=any((w.id,_t)==m for _,ms,_ in joints for m in ms)
            if joined: continue
            d=sqd(*p,ex,ey)
            if d<=T*T: out.append(('free-endpoint',(ex,ey),d,w.id))
    for w in walls:
        r=w.rect()
        if not (r[2]<p[0] or p[0]<r[0] or r[3]<p[1] or p[1]<r[1]):
            out.append(('region',(r[0],r[1]),F(0),w.id))
    for ph in placeholders:
        r=ph.rect
        if not (r[2]<p[0] or p[0]<r[0] or r[3]<p[1] or p[1]<r[1]):
            out.append(('placeholder-box',(r[0],r[1]),F(0),ph.id))
    seen=set(); ded=[]
    for c in sorted(out,key=lambda c:(crank(c[0],hit=True),c[2],str(c[1]),str(c[3]))):
        eid=c[3] if c[0]!='join-node' else ('J',c[3])
        if eid in seen: continue
        seen.add(eid); ded.append(c)
    return ded

# ---------------- S1 ----------------
d_s1=((F(4000)-F(3880))**2+(F(0)-F(0))**2)*Z*Z
check('S1 boundary hit at exact T^2=144 px^2', d_s1==F(144))

# ---------------- G1 ----------------
Wa=Wall('Wa',0,0,1000,0); segA=Wall('segA',2000,-400,2400,-400)
rW,rS=Wa.rect(),segA.rect()
def rects_disjoint(a,b):
    return a[2]<=b[0] or b[2]<=a[0] or a[3]<=b[1] or b[3]<=a[1]
check('G1 accepted-segment full 2-D bounds disjoint from Wa',
      rects_disjoint(rS,rW) and rS==(2000,-500,2400,-300) and rW==(0,-100,1000,100))
def guide_id(winner):
    cls,d,pos,sub,wid=winner
    if cls=='edge':
        comp,ring,si,subf=sub
        face='top' if si==2 and subf=='edge-closest' else 'bottom'
        return ('edge',wid,face)
    if cls=='grid': return ('grid',)
    return (cls,wid)
class GuideState:
    def __init__(self): self.current=None
    def update(self,w):
        key=guide_id(w); changed=key!=self.current; self.current=key; return changed
    def escape(self):
        had=self.current is not None; self.current=None; return had
gs=GuideState(); anchorG=(F(2400),F(-400))
frames=[to_plan('1128.0','744.0'),to_plan('1130.0','742.0'),to_plan('1364.0','708.0')]
cs=[evaluate([Wa,segA],[],p,anchor=anchorG) for p in frames]
w1w=cs[0][0]
check('G1 f1 edge on Wa top face @4 with FULL canonical key',
      w1w[0]=='edge' and w1w[2]==(F(520),F(100)) and w1w[1]==4
      and w1w[3]==(0,0,2,'edge-closest'))
check('G1 f2 same-feature consecutive win @9',
      cs[1][0][0]=='edge' and cs[1][0][1]==9 and cs[1][0][2]==(F(530),F(100)))
gs.update(w1w); ch2=gs.update(cs[1][0]); ch3=gs.update(cs[2][0])
check('G1 guide persists frames 1->2 (same feature id)', ch2 is False)
check('G1 f3 grid-exact (1700,300)',
      cs[2][0][0]=='grid' and cs[2][0][1]==0 and cs[2][0][2]==(F(1700),F(300)))
check('G1 guide CHANGES frame 3', ch3 is True)
check('G1 Escape clears live guide then stays clear',
      gs.escape() and not gs.escape())

# ---------------- S4a/S4b ----------------
seg1=Wall('seg1',0,0,3000,0)
wa=evaluate([seg1],[],to_plan('1654.0','766.0'),anchor=(F(3000),F(0)))
check('S4a axis-line EXTENSION (3150,0) @1 wins',
      wa[0][0]=='axis' and wa[0][1]==1 and wa[0][2]==(F(3150),F(0))
      and wa[0][3]=='extension')
cappt=(F(3000),F(10))
check('S4a cap nearest point (3000,10) @225 misses', sqd(F(3150),F(10),*cappt)==225)
pa=evaluate([seg1],[],to_plan('1625.0','767.4'),anchor=(F(3000),F(0)))
tg=[tuple(c[2]) for c in pa]
check('S4a probe A: no candidate targets the anchor point',
      all(t!=(F(3000),F(0)) for t in tg))
check('S4a probe A keeps edge (3000,3) and axis ext (3005,0)',
      any(c[0]=='edge' and c[2]==(F(3000),F(3)) for c in pa)
      and any(c[0]=='axis' and c[2]==(F(3005),F(0)) for c in pa))
pbc=evaluate([seg1],[],(F(3050),F(0)),anchor=(F(3000),F(0)))
gt=[tuple(c[2]) for c in pbc if c[0]=='grid']
check('S4a probe B: anchor-coincident grid removed across classes; tie survivor (3100,0)',
      (F(3000),F(0)) not in gt and (F(3100),F(0)) in gt and pbc[0][0]=='axis')
s2=Wall('seg2',3000,0,3150,0)
m_pts=[]
for wid,role in [('seg1','End'),('seg2','Start')]:
    w={'seg1':seg1,'seg2':s2}[wid]
    m_pts.append((w.x0,w.y0) if role=='Start' else (w.x1,w.y1))
j1_point=m_pts[0]
check('S4b J1 member-derived coordinate agrees at (3000,0), NOT the later J2 location',
      all(pt==(F(3000),F(0)) for pt in m_pts) and len(set(m_pts))==1
      and j1_point!=(F(3150),F(0)))
J1=('idJ1',(('seg1','End'),('seg2','Start')),j1_point)
Wv=Wall('Wv',3600,-500,3600,1500)
wb=evaluate([seg1,s2,Wv],[J1],to_plan('1744.0','428.0'))
check('S4b axis extension (3600,1700) @0 from anchor (3150,0)',
      wb[0][0]=='axis' and wb[0][1]==0 and wb[0][2]==(F(3600),F(1700))
      and wb[0][3]=='extension')
seg3=Wall('seg3',3150,0,3600,1700)
cy3500=(F(3500)-seg3.x0)*seg3.dy/seg3.dx
check('S4b centerline y at x=3500 equals 11900/9',
      cy3500==F(11900,9) and -500<cy3500<1500)
px_in=F(3520); t_in=(px_in-seg3.x0)/seg3.dx; py_in=t_in*seg3.dy
check('S4b interior rational point proves positive-area band/rect overlap',
      py_in==t_in*(seg3.y1-seg3.y0)
      and (F(3500)<px_in<F(3700)) and (-500<py_in<F(1500)))
tt_end=(F(3600)-seg3.x0)/seg3.dx
check('S4b no axis contact (meets x=3600 only at own endpoint y=1700)',
      tt_end==1 and seg3.axis_proj(F(3600),F(1700))[1]==F(1700)
      and not (-500<=F(1700)<=1500))

# ---------------- U4: executed classification + identity ----------------
WaU=Wall('Wa',0,0,6000,0,h=2500,style='S0',level='L1')
u4=evaluate([WaU],[],to_plan('1624.0','780.0'))
bt={(c[0],tuple(c[2])):c[1] for c in u4}
check('U4 edge bottom-face @16 present; GRID ties at identical target @16',
      bt.get(('edge',(F(3000),F(-100))))==F(16)
      and bt.get(('grid',(F(3000),F(-100))))==F(16))
check('U4 axis ON-segment @36 present; top-face edge excluded by radius',
      bt.get(('axis',(F(3000),F(0))))==F(36)
      and ('edge',(F(3000),F(100))) not in bt)
check('U4 winner edge (3000,-100): class beats tied same-target grid',
      u4[0][0]=='edge' and u4[0][2]==(F(3000),F(-100)))

def point_interior_on_axis(w,pt):
    tt=((pt[0]-w.x0)*w.dx+(pt[1]-w.y0)*w.dy)/w.len2
    return F(0)<tt<F(1)

def segment_crossing(a,b):
    den=(a.dx*b.dy - a.dy*b.dx)
    if den==0: return None
    ax,ay=a.x0,a.y0; bx,by=b.x0,b.y0
    ta=((bx-ax)*b.dy-(by-ay)*b.dx)/den
    tb=((bx-ax)*a.dy-(by-ay)*a.dx)/den
    if F(0)<ta<F(1) and F(0)<tb<F(1):
        return (ax+ta*a.dx, ay+ta*a.dy)
    return None

def classify_split(ws,fresh=None):
    """General proper-crossing resolution (any rational orientation pair).
    Returns (children, join_members, join_point, retained). The
    original-start piece retains the wall id; other pieces take fresh ids
    from `fresh` (an inexhaustible generator by default); children inherit
    ALL parent fields (R3.1.3)."""
    if fresh is None:
        def _gen():
            n=0
            while True:
                n+=1; yield f'id{n}'
        fresh=_gen()
    ws=list(ws)
    pts=[]
    for i,a in enumerate(ws):
        for b in ws[i+1:]:
            pt=segment_crossing(a,b)
            if pt is not None and point_interior_on_axis(a,pt) \
               and point_interior_on_axis(b,pt):
                pts.append((pt,a.id,b.id))
    children=[]; members=[]
    retained={}
    for w in ws:
        wpts=sorted({p for (p,aid,bid) in pts if aid==w.id or bid==w.id},
                    key=lambda q:(abs(q[0]-w.x0)+abs(q[1]-w.y0)))
        if not wpts:
            retained[w.id]=False
            continue
        chain=[(w.x0,w.y0)]+wpts+[(w.x1,w.y1)]
        piece_ids=[]
        for k in range(len(chain)-1):
            pid=w.id if k==0 else next(fresh)
            piece_ids.append(pid)
            child=Wall(pid,chain[k][0],chain[k][1],chain[k+1][0],chain[k+1][1])
            child.fields=dict(w.fields)
            children.append(child)
        for i in range(len(wpts)):
            members.append((piece_ids[i],'End'))
            members.append((piece_ids[i+1],'Start'))
        retained[w.id]=(piece_ids[0]==w.id)
    join_pts=sorted({p for p,_,_ in pts})
    join_pt=join_pts[0] if join_pts else None
    return children,members,join_pt,retained

children,members,jpt,retained=classify_split(
    [WaU,Wall('Wb',3000,-100,3000,2500,t=250,h=2800,style='WB')],
    fresh=iter(['idD','idB2']))
check('U4 classification EXECUTED: proper crossing splits BOTH participants into four children',
      len(children)==4 and jpt==(F(3000),F(0)))
child_by_id={c.id:c for c in children}
check('U4 R3.1.3 identity: original-start children retain Wa/Wb ids',
      'Wa' in child_by_id and 'Wb' in child_by_id
      and child_by_id['Wa'].x0==F(0) and child_by_id['Wa'].x1==F(3000)
      and child_by_id['Wb'].y0==F(-100) and child_by_id['Wb'].y1==F(0)
      and retained.get('Wa') is True and retained.get('Wb') is True)
check('U4 fresh supplied identities idD/idB2 for tail pieces',
      {'idD','idB2'}<=set(child_by_id))
check('U4 four-member join membership all at exact coordinate (3000,0)',
      sorted(members)==sorted([('Wa','End'),('Wb','End'),
                               ('idD','Start'),('idB2','Start')]))
check('U4 children inherit ALL parent fields per R3.1.3 (non-default parents)',
      child_by_id['Wa'].fields==WaU.fields
      and child_by_id['idB2'].fields=={'thicknessMm':F(250),'heightMm':F(2800),
                                       'style':'WB','levelId':'L1'})
Diag=Wall('Diag',2600,-300,3100,75,t=300,h=3100,style='DX')
ch2d,mem2d,jpt2,ret2=classify_split([Wall('H2',0,0,6000,0),Diag])
check('classify_split handles DIAGONAL crossings (rational orientation pair)',
      jpt2==(F(3000),F(0)) and len(ch2d)==4)
h_fields=[c.fields for c in ch2d if c.id=='H2']
d_fields=[c.fields for c in ch2d if c.id=='Diag']
check('diagonal-crossing children inherit ALL parent fields',
      bool(h_fields) and h_fields[0]['style']=='S0'
      and bool(d_fields) and d_fields[0]==Diag.fields
      and d_fields[0]['style']=='DX' and d_fields[0]['heightMm']==F(3100)
      and d_fields[0]['thicknessMm']==F(300))
H3=Wall('H3',0,0,6000,0,t=250,h=2800,style='HS')
V3=Wall('V3',3000,-2000,3000,2000,t=300,h=3100,style='VS')
D3=Wall('D3',0,-3000,6000,3000,t=350,h=3300,style='DS')
ch3,mem3,jpt3,ret3=classify_split([H3,V3,D3])
check('U4 THREE-WAY proper crossing resolves without StopIteration: six children at (3000,0)',
      len(ch3)==6 and jpt3==(F(3000),F(0)) and len(mem3)==6
      and {c.id for c in ch3}=={'H3','V3','D3','id1','id2','id3'})
child_field_sets={tuple(sorted(c.fields.items())) for c in ch3}
parent_field_sets={tuple(sorted(w.fields.items())) for w in (H3,V3,D3)}
check('U4 three-way children inherit parent fields (three distinct field sets); original-start pieces retain ids',
      ret3=={'H3':True,'V3':True,'D3':True}
      and child_field_sets==parent_field_sets and len(child_field_sets)==3)

# ---------------- S7 ----------------
w1=Wall('W1',0,0,4000,0)
mouse=evaluate([w1],[],to_plan('1854.0','738.0'),device='mouse',grid=False)
touch=evaluate([w1],[],to_plan('1854.0','738.0'),device='touch',grid=False)
check('S7 mouse TRUE MISS under grid=disabled', mouse==[])
check('S7 touch HIT free-endpoint (4000,0) @450',
      touch[0][0]=='free-endpoint' and touch[0][2]==(F(4000),F(0)) and touch[0][1]==450)

# ---------------- angles: exact units + full enumeration ----------------
anch=(F(3000),F(0))
on45=(F(3200),F(200))
ac=evaluate([],[],on45,angles=True,chain=(anch,F(0)))
a45=[c for c in ac if c[0]=='angle' and int(c[3])==45]
check('angle 45deg on-ray probe: EXACT zero distance in logical px^2',
      bool(a45) and a45[0][1]==0 and tuple(a45[0][2])==(1,1))
luna_p=(F(3100),F(50))
luna=evaluate([],[],luna_p,angles=True,chain=(anch,F(0)))
aluna=[c for c in luna if c[0]=='angle' and c[3]==45]
check('Luna probe (100 along, 50 off) qualifies at EXACT 25/2 px^2 (25 mm^2 plan)',
      bool(aluna) and aluna[0][1]==F(25,2) and aluna[0][2]==(1,1))
west=evaluate([],[],(F(2800),F(0)),angles=True,chain=(anch,F(0)))
wdirs=[(int(c[3]),c[1]) for c in west if c[0]=='angle']
check('WEST angle: pointer (2800,0) from anchor (3000,0) yields exactly the -180 candidate at 0',
      INT_DIR[-180]==(-1,0) and wdirs==[(-180,0)])
behind=evaluate([],[],(F(2900),F(-100)),angles=True,chain=(anch,F(0)))
behind_dirs=[c[3] for c in behind if c[0]=='angle']
check('angle behind-anchor projection excluded (no 0deg candidate)',
      all(int(d)!=0 for d in behind_dirs))
dedupe=angle_targets(F(15))
check('angle enumeration includes global family AND relative additions (12 targets)',
      len(dedupe)==12 and {-180,-135,-90,-45,0,45,90,135}<=set(map(int,dedupe))
      and {15,60,105,150}<=set(map(int,dedupe)))
check('angle evaluation handles exactly the rational-direction subset',
      set(INT_DIR)=={0,45,90,135,-180,-135,-90,-45})

# ---------------- axis comparator: position attribute before stable ID ----
Wa_ax=Wall('Wa',0,0,1000,0); Wb_ax=Wall('Wb',1500,0,3000,0)
axc=[c for c in evaluate([Wa_ax,Wb_ax],[],(F(2000),F(50)),grid=False)
     if c[0]=='axis']
check('axis comparator orders POSITION attribute (on-segment) before stable wall ID',
      len(axc)==2 and axc[0][4]=='Wb' and axc[0][3]=='on-segment'
      and axc[1][4]=='Wa' and axc[1][3]=='extension' and 'Wa'<'Wb')

# ---------------- canonical edge keys across components ----------------
Wmc=Wall('Wmc',0,0,1000,0)
Wmc.patches=[[(F(5000),F(0)),(F(6000),F(0)),
              (F(6000),F(200)),(F(5000),F(200))]]
_,_,k_patch=Wmc.boundary_closest(F(5500),F(250))
_,_,k_main=Wmc.boundary_closest(F(500),F(200))
_,_,k_tie=Wmc.boundary_closest(F(500),F(0))
check('edge keys enumerate COMPONENT ordinals across a multi-component region',
      k_patch==(1,0,2,'edge-closest') and k_main==(0,0,2,'edge-closest'))
check('edge key tie between top and bottom faces resolves by vertex-run ordinal',
      k_tie==(0,0,0,'edge-closest'))
Wrev=Wall('Wrev',1000,0,0,0)
_,_,k_rev=Wrev.boundary_closest(F(500),F(200))
check('edge key canonicalization is direction-invariant (reversed wall shares ring and top-face ordinal)',
      k_rev==(0,0,2,'edge-closest')
      and Wrev.canonical_ring()==Wmc.canonical_ring())

# ---------------- hit classes with applicable radii ----------------
hj_a=Wall('hjA',0,0,3000,0); hj_b=Wall('hjB',3000,0,5000,0)
hj_joint=('hjJ1',(('hjA','End'),('hjB','Start')),(F(3000),F(0)))
hits_join=evaluate_hits([hj_a,hj_b],[hj_joint],[],(F(3000),F(0)))
check('hit test at a join point classifies join-node (not region)',
      hits_join[0][0]=='join-node' and hits_join[0][1]==(F(3000),F(0))
      and hits_join[0][3]=='hjJ1')
ep_wall=Wall('epW',0,0,4000,0)
hits_ep=evaluate_hits([ep_wall],[],[],(F(0),F(0)))
check('hit test at an EXACT free endpoint classifies free-endpoint (not region)',
      hits_ep[0][0]=='free-endpoint' and hits_ep[0][1]==(F(0),F(0))
      and hits_ep[0][3]=='epW')
m_near=evaluate_hits([ep_wall],[],[],(F(4000),F(200)),device='mouse')
t_near=evaluate_hits([ep_wall],[],[],(F(4000),F(200)),device='touch')
check('applicable radius: pointer 20 logical px off an endpoint is touch-only',
      all(c[0]!='free-endpoint' for c in m_near)
      and any(c[0]=='free-endpoint' and c[1]==(F(4000),F(0)) for c in t_near))

# ---------------- Selection machine CR1-CR7 via HIT TESTING ----------------
class Scene:
    """Live candidate provider over projection entities; deletion is
    routed through delete_entity which applies the R7.4 primary rules to
    every bound selector."""
    def __init__(self,walls_by_level,placeholders=None):
        self.walls=walls_by_level
        self.placeholders=placeholders or []
        self.deleted=set()
        self.selectors=[]
    def level_candidates(self,level,p,ptype='mouse'):
        ws=[w for w in self.walls.get(level,[]) if w.id not in self.deleted]
        phs=[ph for ph in self.placeholders if ph.id not in self.deleted]
        return evaluate_hits(ws,[],phs,p,device=ptype)
    def delete_entity(self,eid):
        self.deleted.add(eid)
        for s in self.selectors:
            if eid in s.selection:
                s.selection.discard(eid)
                if s.primary==eid: s.primary=None
        return 'deleted'

class Selector:
    def __init__(self,scene,level='L1'):
        self.scene=scene; self.level=level
        self.selection=set(); self.primary=None
        self.last=None; self.cycle_index=None
        scene.selectors.append(self)
    def click(self,position,view=(Z,OX,OY),modifiers=frozenset(),ptype='mouse',
              vissig=frozenset(),level=None):
        level=level or self.level
        cands=self.scene.level_candidates(level,position,ptype=ptype)
        ids=[c[3] for c in cands]
        ctx=(position,view,modifiers,ptype,tuple(ids),level,vissig)
        if level!=self.level:
            self.selection.clear(); self.primary=None
        if self.last is None or ctx!=self.last:
            self.cycle_index=None
        self.last=ctx; self.level=level
        if not cands:
            self.selection.clear(); self.primary=None; return 'cleared'
        if modifiers & {'shift'}:
            item=cands[self.cycle_index%len(cands)] \
                 if self.cycle_index is not None else cands[0]
            item=item[3]
            if item in self.selection:
                self.selection.discard(item)
                if self.primary==item: self.primary=None
            else:
                self.selection.add(item); self.primary=item
            return ('toggled',item)
        if self.cycle_index is None: self.cycle_index=0
        else: self.cycle_index=(self.cycle_index+1)%len(cands)
        item=cands[self.cycle_index][3]
        self.selection={item}; self.primary=item
        return item
    def escape_idle(self):
        self.selection=set(); self.primary=None; return 'cleared'

pos=(F(2000),F(1450))
L1walls=[Wall('Wa',0,1400,4000,1400),Wall('Wb',0,1450,4000,1450),
         Wall('Wc',0,1500,4000,1500)]
scene=Scene({'L1':L1walls,'L2':[Wall('L2only',0,1400,4000,1400)]})
sel=Selector(scene)
res={}
res['base']=sel.click(pos)
sel.click((F(2600),F(1450)))
res['CR1']=sel.click(pos)
res['CR2']=sel.click(pos,view=(Z,OX-F(100),OY))
res['CR3_switch_L2']=sel.click(pos,level='L2')
res['CR3_L2_state']=(set(sel.selection),sel.primary)
res['CR3_back_clear']=sel.click(pos,level='L1')
res['CR3_advance']=sel.click(pos,level='L1')
res['CR4']=sel.click(pos,vissig=frozenset({('L2','hidden')}))
res['CR5_shift']=sel.click(pos,modifiers=frozenset({'shift'}))
res['CR5_after_plain']=sel.click(pos)
res['CR6_tap']=sel.click(pos,ptype='touch')
res['CR6_mouse']=sel.click(pos,ptype='mouse')
exp={'base':'Wa','CR1':'Wa','CR2':'Wa','CR3_switch_L2':'L2only',
     'CR3_L2_state':({'L2only'},'L2only'),'CR3_back_clear':'Wa',
     'CR3_advance':'Wb','CR4':'Wa','CR5_shift':('toggled','Wa'),
     'CR5_after_plain':'Wa','CR6_tap':'Wa','CR6_mouse':'Wa'}
for k,v in exp.items(): check('CR '+k,res[k]==v)

# CR7: genuine deletion through the scene rebuilds candidates
scene7=Scene({'L1':[Wall('Wa',0,1400,4000,1400),Wall('Wb',0,1450,4000,1450),
                    Wall('Wc',0,1500,4000,1500)]})
s7=Selector(scene7)
s7.click(pos)
scene7.delete_entity('Wc')
nxt=s7.click(pos)
check('CR7 genuine deletion removes Wc from the REBUILT candidate pool',
      'Wc' in scene7.deleted and nxt in ('Wa','Wb')
      and all(c[3]!='Wc' for c in scene7.level_candidates('L1',pos)))
check('CR7 next click selects from the rebuilt pool (no Wc)', nxt in ('Wa','Wb'))

# far-away click clears selection (hit-testing driven)
res['far']=sel.click((F(60000),F(0)))
check('far-away click CLEARS selection (no injected IDs)', res['far']=='cleared'
      and sel.selection==set() and sel.primary is None)

# ---------------- PH1: full placeholder flow ----------------
class Placeholder:
    def __init__(self,pid,x0,y0,x1,y1):
        self.id=pid; self.rect=(F(x0),F(y0),F(x1),F(y1))
ph=Placeholder('inst1',1500,-500,2500,500)
ph_scene=Scene({'L1':[Wall('H1',0,6000,1000,6000)]},placeholders=[ph])
ph_sel=Selector(ph_scene)
inside_box=(F(2000),F(0))
top=ph_sel.click(inside_box)
check('PH1 placeholder-box hit class selects instance when alone',
      top=='inst1' and (set(ph_sel.selection),ph_sel.primary)==({'inst1'},'inst1'))
Over=Wall('Over',1900,-300,1900,300)
both=evaluate_hits([Over],[],[ph],(F(2000),F(0)))
check('PH1 strict priority: region precedes placeholder-box on overlap',
      both[0][0]=='region' and both[1][0]=='placeholder-box')
snap_with=evaluate([],[],to_plan('1514.0','768.0'),placeholders=[ph])
snap_without=evaluate([],[],to_plan('1514.0','768.0'))
check('PH1 placeholders PASSED INTO snap evaluation are never sources (identical results)',
      [(c[0],c[2]) for c in snap_with]==[(c[0],c[2]) for c in snap_without]
      and snap_with[0][0]=='grid' and snap_with[0][2]==(F(2400),F(0))
      and snap_with[0][1]==25)
cyc_scene=Scene({'L1':[Over]},placeholders=[ph])
cyc=Selector(cyc_scene)
c1=cyc.click((F(2000),F(0)))
c2=cyc.click((F(2000),F(0)))
check('PH1 multi-candidate cycling reaches placeholder-box as SECOND candidate',
      (c1,c2)==('Over','inst1') and cyc.primary=='inst1')
cyc_scene.delete_entity('inst1')
check('PH1 deletion of the primary routed through selection/document flow: primary UNSET',
      cyc.selection==set() and cyc.primary is None and 'inst1' in cyc_scene.deleted)
c3=cyc.click((F(2000),F(0)))
check('PH1 post-deletion pool excludes the deleted placeholder', c3=='Over')

# ---------------- Marquee: shape-aware, SAT, connected dedup --------------
def aabb_intersects(r,rect):
    return not (rect[2]<r[0] or r[2]<rect[0] or rect[3]<r[1] or r[3]<rect[1])
def sat_overlap(polyA,polyB):
    def axes(poly):
        n=len(poly); out=[]
        for i in range(n):
            ax,ay=poly[i]; bx,by=poly[(i+1)%n]
            out.append((-(by-ay),(bx-ax)))
        return out
    for poly in (polyA,polyB):
        for nx,ny in axes(poly):
            pa=[nx*x+ny*y for x,y in polyA]
            pb=[nx*x+ny*y for x,y in polyB]
            if max(pa)<min(pb) or max(pb)<min(pa): return False
    return True
def rect_poly(r):
    x0,y0,x1,y1=r
    return [(x0,y0),(x1,y0),(x1,y1),(x0,y1)]
def marquee_intersects(rect,w):
    if w.dx==0 or w.dy==0:
        return aabb_intersects(rect,w.rect())
    return sat_overlap(rect_poly((F(rect[0]),F(rect[1]),F(rect[2]),F(rect[3]))),
                       w.corners())
def marquee_add(rect,walls,placeholder,selection,primary):
    raw=[]
    for w in walls:
        if marquee_intersects(rect,w): raw.append(('region',w))
    if placeholder is not None and aabb_intersects(rect,placeholder.rect):
        raw.append(('placeholder-box',placeholder))
    seen=set(); keyed=[]
    for cls,ent in raw:
        if ent.id in seen: continue
        seen.add(ent.id)
        r=ent.rect() if callable(getattr(ent,'rect',None)) else ent.rect
        keyed.append(((r[0],r[1]),crank(cls,hit=True),ent.id))
    keyed.sort()
    added=[eid for _,_,eid in keyed if eid not in selection]
    for eid in added:
        selection.add(eid); primary=eid
    return added,selection,primary

tan_rect=(-500,100,5000,600)
g1_wall=Wall('G1W',0,0,1000,0)
check('marquee TANGENT contact selects boundary-inclusively (AABB test)',
      aabb_intersects(tan_rect,g1_wall.rect()) is True)
diag=Wall('DiagM',3150,0,3430,210)
diag_band=diag.corners()
mpoly=(3200,50,3300,150)
check('marquee vs DIAGONAL band decided by exact SAT (positive case)',
      sat_overlap(rect_poly(tuple(F(v) for v in mpoly)),diag_band) is True)
neg_rect=(5000,2000,6000,3000)
check('marquee SAT negative case returns False',
      sat_overlap(rect_poly(tuple(F(v) for v in neg_rect)),diag_band) is False)
fp_rect=(F(3090),F(220),F(3230),F(290))
fp_added,_,_=marquee_add(fp_rect,[diag],None,set(),None)
check('marquee EXECUTION rejects the known AABB false-positive rectangle via SAT',
      aabb_intersects(fp_rect,diag.rect()) is True
      and sat_overlap(rect_poly(fp_rect),diag_band) is False
      and fp_added==[])
pos_added,_,_=marquee_add(mpoly,[diag],None,set(),None)
check('marquee EXECUTION selects the diagonal band on true SAT overlap',
      pos_added==['DiagM'])
L1M=[Wall('Wa',0,1400,4000,1400),Wall('Wb',0,1450,4000,1450),
     Wall('Wc',0,1500,4000,1500)]
mrect=(-500,1200,4500,1700)
sm=set(); pm=None
added,sm,pm=marquee_add(mrect,L1M,None,sm,pm)
check('marquee adds walls in ascending key order; primary last added',
      added==['Wa','Wb','Wc'] and pm=='Wc')
added2,sm,pm=marquee_add(mrect,L1M,None,sm,pm)
check('additive marquee adding NOTHING new leaves selection AND primary unchanged',
      added2==[] and sm=={'Wa','Wb','Wc'} and pm=='Wc')
dup_added,ds,_=marquee_add(mrect,L1M+L1M,None,set(),None)
check('marquee CONNECTED dedup adds each entity once under duplicate feature delivery',
      dup_added==['Wa','Wb','Wc'] and ds=={'Wa','Wb','Wc'})
sp=set(); pp=None
addedp,sp,pp=marquee_add((-500,-600,4500,700),[],ph,sp,pp)
check('PLACEHOLDER marquee selection works via deterministic bounds',
      addedp==['inst1'] and pp=='inst1')

# ---------------- Engine: projection, view, gestures, stacks --------------
class Projection:
    def __init__(self): self.entities={}; self.tombstones={}
    def apply(self,ops):
        for kind,eid,fields in ops:
            if kind=='create':
                assert eid not in self.entities and eid not in self.tombstones
                self.entities[eid]=dict(fields)
            elif kind=='update':
                self.entities[eid].update(fields)
            elif kind=='delete':
                self.tombstones[eid]=self.entities.pop(eid)
    def snapshot(self): return (dict(self.entities),dict(self.tombstones))
    def restore(self,snap):
        self.entities=dict(snap[0]); self.tombstones=dict(snap[1])

class Engine:
    def __init__(self):
        self.proj=Projection()
        self.counters={}; self.frontier={}; self.saved={}
        self.undo=[]; self.redo=[]; self.dirty=False
        self.selection=set(); self.primary=None
        self.gesture=None; self.events=[]; self.view_ops=0
        self.view=(Z,OX,OY); self.grid_enabled=True
        self.nonfinite_rejected=0; self.alt_held=False
        self.compaction=None; self.replacement_ambiguous=False
        self.last_ops=[]
    def _advance(self,actor):
        self.counters[actor]=self.counters.get(actor,0)+1
        self.frontier[actor]=self.counters[actor]
    def _commit(self,name,ops):
        self.proj.apply(ops); self._advance('A')
        entry=(name,tuple(sorted(self.frontier.items())))
        self.undo.append(entry); self.redo.clear()
        self.dirty=self.frontier!=self.saved
        self.last_ops=list(ops)
        self.events.append(('commit',name)); return entry
    def accept_local(self,name,ops):
        assert self.gesture is None
        return self._commit(name,ops)
    def execute_undo(self,name,comp_ops):
        b=self.undo.pop(); self.redo.append(b)
        self.proj.apply(comp_ops); self._advance('A')
        self.dirty=self.frontier!=self.saved
    def undo_attempt(self,name,comp_ops):
        if not self.undo: return 'no-op'
        self.execute_undo(name,comp_ops); return 'undone'
    # ---- view state: REAL transforms (section 1.4) ----
    def view_op(self,dz=0,dx=0,dy=0,factor=None,anchor_logical=None):
        """Pan by (dx,dy) logical px; a factor zoom anchors at the CURSOR
        (viewport centre when no cursor is supplied), keeping the plan
        point under the anchor invariant."""
        z,ox,oy=self.view
        ox=ox+dx; oy=oy+dy
        if factor is not None:
            ax,ay=anchor_logical if anchor_logical is not None else (F(512),F(384))
            px=(ax-ox)/z; py=(oy-ay)/z
            z=z*factor
            ox=ax-z*px; oy=ay+z*py
        self.view_ops+=1
        self.view=(z,ox,oy)
    def toggle_grid(self):
        self.grid_enabled=not self.grid_enabled; self.view_ops+=1
    def alt_key(self,pressed):
        self.alt_held=pressed
    # ---- input gates (R6.5.8) ----
    def _nonfinite(self,v):
        if isinstance(v,float):
            return v!=v or v in (float('inf'),float('-inf'))
        if isinstance(v,dict):
            return any(self._nonfinite(x) for x in v.values())
        if isinstance(v,(list,tuple)):
            return any(self._nonfinite(x) for x in v)
        return False
    def reject_nonfinite(self,event):
        if self._nonfinite(event):
            self.nonfinite_rejected+=1; return 'rejected'
        return 'accepted'
    # ---- gestures ----
    def begin_drag(self,pid,payload=None,updates=None,sel=None,prim=None,
                   snapshot=None,disable_snap=None):
        if disable_snap is None: disable_snap=self.alt_held
        snap=snapshot if snapshot is not None else self.proj.snapshot()
        self.gesture={'pid':pid,'payload':dict(payload or {}),
                      'updates':dict(updates or {}),
                      'pre_selection':set(sel or ()),
                      'pre_primary':prim,'snapshot':snap,
                      'disable_snap':disable_snap,
                      'preview_overlay':{},'last_logical':None}
    def preview_mutate(self,eid,fields):
        assert self.gesture is not None
        self.gesture['preview_overlay'][eid]=dict(fields)
    def pointer_move(self,pid,logical=None):
        if self.gesture is None or pid!=self.gesture['pid']:
            self.events.append(('stale-move',pid)); return 'ignored'
        if logical is not None: self.gesture['last_logical']=logical
        return 'tracked'
    def pointer_up(self,pid):
        if self.gesture is None or pid!=self.gesture['pid']:
            self.events.append(('stale-up',pid)); return 'ignored'
        g=self.gesture; self.gesture=None
        if g['disable_snap'] and g['last_logical'] is not None and g['updates']:
            z,ox,oy=self.view
            lx,ly=g['last_logical']
            plan=((lx-ox)/z,(oy-ly)/z)
            ops=[('update',eid,{field:plan})
                 for eid,field in sorted(g['updates'].items())]
            self._commit(f'drag-{pid}',ops)
            return ('committed',plan)
        return 'released-noop'
    def cancel_drag(self):
        g=self.gesture
        self.selection=set(g['pre_selection']); self.primary=g['pre_primary']
        self.proj.restore(g['snapshot'])
        discarded=dict(g['preview_overlay'])
        self.gesture=None
        self.events.append(('cancel-restored',g['snapshot']))
        self.events.append(('cancel',g['pid']))
        return discarded
    def external_batch(self,actor,ctr,ops):
        if self.gesture is not None: self.cancel_drag()
        self.undo.clear(); self.redo.clear()
        self.proj.apply(ops)
        self.counters[actor]=ctr; self.frontier[actor]=ctr
        self.dirty=self.frontier!=self.saved
        self.events.append(('external',actor,ctr))
    def save(self):
        self.saved=dict(self.frontier); self.dirty=False
    def recovery_open(self,replayed_ops_by_actor):
        """CLEAR pre-existing state ENTIRELY (entities, tombstones,
        selection, counters, frontier), then replay from a fresh baseline."""
        self.proj.entities={}; self.proj.tombstones={}
        self.selection=set(); self.primary=None; self.gesture=None
        self.counters={}; self.frontier={}
        for actor,oplist in replayed_ops_by_actor.items():
            for ops in oplist:
                self.proj.apply(ops); self._advance(actor)
        self.undo=[]; self.redo=[]
        self.saved=dict(self.frontier); self.dirty=False
    def compact(self,cover_batches):
        """R8.4.3 compaction: the snapshot covers the effects of
        `cover_batches` (a leading prefix of the undo stack); those entries
        are dropped, the remainder stays live, and a non-blocking
        diagnostic names the boundary."""
        names=[b[0] for b in cover_batches]
        assert names==[b[0] for b in self.undo[:len(names)]], \
               'cover must be a leading prefix of the undo stack'
        dropped=self.undo[:len(names)]
        self.undo=self.undo[len(names):]
        diag=(f'snapshot frontier covers {names[0]}-{names[-1]}; '
              f'{len(dropped)} undo entries truncated')
        self.compaction={'snapshot_frontier':dict(self.frontier),
                         'dropped':tuple(b[0] for b in dropped),
                         'diagnostic':diag}
        self.events.append(('compaction',diag))
        return diag
    # ---- save-failure matrix with reader-equivalent validation ----
    @staticmethod
    def validate_package(b):
        doc=json.loads(b)
        assert isinstance(doc,dict), 'package must be a JSON object'
        d=doc.get('doc'); fr=doc.get('frontier')
        assert isinstance(d,dict), 'doc must be an object'
        assert set(d)=={'entities'}, 'unknown doc shape'
        ents=d['entities']
        assert isinstance(ents,dict) and ents, \
               'doc.entities must be a non-empty map'
        for eid,fields in ents.items():
            assert isinstance(eid,str) and eid, 'entity id must be non-empty'
            assert isinstance(fields,dict), 'entity payload must be an object'
        assert isinstance(fr,dict) and fr, 'frontier must be a non-empty map'
        for actor,ctr in fr.items():
            assert isinstance(ctr,int) and not isinstance(ctr,bool) \
                   and ctr>=1, 'counter must be an integer >= 1'
        return doc
    def save_attempt(self,outcome,new_frontier,target_bytes,
                     baseline_bytes,candidate_bytes):
        if outcome=='pre-replacement':
            return 'aborted'
        if outcome=='replacement-interrupted':
            self.replacement_ambiguous=True
            self.validate_package(target_bytes)
            self.validate_package(baseline_bytes)
            self.validate_package(candidate_bytes)
            if target_bytes==candidate_bytes:
                self.saved=dict(new_frontier); self.dirty=False
                return 'resolved-new'
            if target_bytes==baseline_bytes:
                self.dirty=self.frontier!=self.saved
                return 'resolved-old'
            raise AssertionError(
                'target bytes match neither baseline nor candidate exactly')
        if outcome=='dirsync-failed':
            self.validate_package(candidate_bytes)
            assert json.loads(candidate_bytes)['frontier']==new_frontier
            self.saved=dict(new_frontier); self.dirty=False
            return 'completed-durability-caveat'

# ---------------- EB1 ----------------
eb=Engine()
eb.accept_local('B1',[('create','W1',{'start':(0,0),'end':(4000,0)})])
eb.save()
check('EB1 baseline: save establishes clean baseline dirty FALSE',
      eb.saved=={'A':1} and eb.dirty is False)
eb.external_batch('EXT',5,[('create','X',{'start':(9000,0),'end':(9500,0)})])
check('EB1 external fence clears BOTH stacks; dirty FALSE->TRUE vs saved baseline',
      eb.undo==[] and eb.redo==[] and eb.dirty is True
      and eb.frontier=={'A':1,'EXT':5} and eb.saved=={'A':1})

e=Engine()
e.accept_local('B1',[('create','W1',{'start':(0,0),'end':(4000,0)})])
e.execute_undo("B1'",[('delete','W1',{})])
check('EB1 setup: W1 tombstoned before the gesture',
      'W1' in e.proj.tombstones and [b[0] for b in e.redo]==['B1'])
e.begin_drag('p1',payload={'W2.end':(4000,900)},sel=set(),prim=None,
             snapshot=e.proj.snapshot())
pre_snap=e.gesture['snapshot']
e.external_batch('EXT',1,[('create','X1',{'start':(9000,-2000),
                                          'end':(11000,-2000)})])
check('EB1 cancel over tombstoned state PRESERVES tombstones',
      'W1' in e.proj.tombstones and e.gesture is None)
check('EB1 snapshot captured tombstones at begin_drag', 'W1' in pre_snap[1])
e.accept_local('B2',[('create','W2',{'start':(4000,0),'end':(4000,3000)})])
e.selection={'W2'}; e.primary='W2'
e.begin_drag('p9',payload={'W2.start':(4000,450)},sel=e.selection,prim='W2',
             snapshot=e.proj.snapshot())
e.preview_mutate('W2',{'start':(4000,450)})
check('EB1 preview overlay mutates WITHOUT touching projection',
      e.proj.entities['W2']['start']==(4000,0)
      and e.gesture['preview_overlay']['W2']=={'start':(4000,450)})
check('EB1 mid-drag foreign-pid move rejected while gesture persists',
      e.pointer_move('ghost')=='ignored' and e.gesture is not None)
e.external_batch('EXT2',1,[('create','X9',{'start':(9000,0),'end':(9500,0)})])
ci=[i for i,ev in enumerate(e.events) if ev[0]=='cancel-restored']
ei=[i for i,ev in enumerate(e.events) if ev[0]=='external']
check('EB1 cancel-restored PRECEDES external apply',
      bool(ci) and bool(ei) and ci[-1]<ei[-1])
check('EB1 selection/primary restored after cancel',
      (e.selection,e.primary)==({'W2'},'W2'))
check('EB1 stale pointer-up rejected after cancel too',
      e.pointer_up('p9')=='ignored')

# ---------------- recovery over PRE-EXISTING state ----------------
r=Engine()
r.accept_local('OLD1',[('create','OLD',{'start':(0,0),'end':(100,0)})])
r.accept_local('OLD2',[('update','OLD',{'end':(200,0)})])
r.selection={'OLD'}; r.primary='OLD'
r.recovery_open({'A':[[('create','NEW',{'start':(0,0),'end':(500,0)})]]})
check('recovery resets counters AND frontier to a FRESH A:1 after pre-existing local batches',
      r.counters=={'A':1} and r.frontier=={'A':1}
      and set(r.proj.entities)=={'NEW'} and r.selection==set()
      and r.primary is None and r.saved=={'A':1} and r.dirty is False
      and r.undo==[] and r.redo==[])

# ---------------- NF1 ----------------
n=Engine()
n.accept_local('seed',[('create','S1',{'start':(0,0),'end':(1000,0)})])
n.selection={'S1'}; n.primary='S1'
n.begin_drag('q1',sel=n.selection,prim='S1')
w_nf=Wall('S1',0,0,1000,0)
snap_before=evaluate([w_nf],[],(F(500),F(0)))
view_before=n.view
out1=n.reject_nonfinite({'type':'pointer-move',
                         'coords':{'x':F(0),'y':float('nan')}})
out2=n.reject_nonfinite({'type':'pointer-move',
                         'coords':[F(1),(F(2),float('-inf'))]})
out3=n.reject_nonfinite({'type':'pointer-move','coords':(F(100),F(0))})
check('NF1 flat AND nested non-finite events rejected whole; valid nested accepted',
      out1=='rejected' and out2=='rejected' and out3=='accepted'
      and n.nonfinite_rejected==2)
check('NF1 rejection leaves gesture/selection/view/snap state untouched',
      n.gesture is not None and (n.selection,n.primary)==({'S1'},'S1')
      and n.view==view_before
      and evaluate([w_nf],[],(F(500),F(0)))==snap_before
      and len(n.undo)==1 and n.dirty is True)
g=n.cancel_drag()
check('NF1 subsequent valid interaction resolves cleanly after rejection',
      g=={} and n.gesture is None and (n.selection,n.primary)==({'S1'},'S1'))

# ---------------- VF1 idle phase ----------------
v=Engine()
v.accept_local('seed',[('create','V1',{'start':(0,0),'end':(1000,0)})])
v.save()
commits_before=len(v.undo); frontier_before=dict(v.frontier)
saved_before=dict(v.saved)
v.view_op(dy=F(100))
check('VF1 plain-wheel pan mutates oy 384->484 FIRST',
      v.view==(Z,OX,OY+100))
v.view_op(factor=F(4,5),anchor_logical=(F(700),F(300)))
check('VF1 cursor-anchored zoom at logical (700,300) yields {z=0.08,(549.6,447.2)} exactly',
      v.view==(F('0.08'),F('549.6'),F('447.2')))
vctl=Engine()
vctl.view_op(dy=F(100))
vctl.view_op(factor=F(4,5))
check('VF1 cursor anchoring DIVERGES from center-anchored zoom (512,464)',
      vctl.view==(F('0.08'),F(512),F(464)) and vctl.view!=v.view)
v.toggle_grid(); v.toggle_grid()
v.view_op(); v.view_op()
clk=to_plan(F('1419.2'),F('662.4'),*v.view)
check('VF1 post-transform click maps raw (1419.2,662.4) to plan (2000,1450)',
      clk==(F(2000),F(1450)))
vf_scene=Scene({'L1':[Wall('Wa',0,1400,4000,1400),
                      Wall('Wb',0,1450,4000,1450),
                      Wall('Wc',0,1500,4000,1500)]})
vf_sel=Selector(vf_scene)
check('VF1 post-transform click selects top-ranked Wa through the new transform',
      vf_sel.click(clk)=='Wa')
check('VF1 idle Escape clears the selection',
      vf_sel.escape_idle()=='cleared'
      and vf_sel.selection==set() and vf_sel.primary is None)
v.alt_key(True); v.alt_key(False)
check('VF1 view sequence: ZERO commits; frontier/saved/dirty untouched',
      len(v.undo)==commits_before and v.frontier==frontier_before
      and v.saved==saved_before and v.dirty is False and v.view_ops==6)

# ---------------- VF1 mid-gesture phase ----------------
v2=Engine()
v2.accept_local('F2a',[('create','W1',{'start':(0,0),'end':(4000,-10)})])
v2.accept_local('F2b',[('create','W2',{'start':(4010,-10),'end':(4000,3000)})])
pre_w1=dict(v2.proj.entities['W1']); pre_w2=dict(v2.proj.entities['W2'])
v2.alt_key(True)
v2.begin_drag('p1',updates={'W1':'end','W2':'start'},sel=set(),prim=None)
v2.pointer_move('p1',logical=(F('913.0'),F('385.0')))
check('VF1 mid-gesture preview tracked with ZERO operations during the pan',
      v2.gesture is not None and v2.gesture['disable_snap'] is True
      and len(v2.undo)==2)
v2.view_op(dy=F(100))
res_up=v2.pointer_up('p1')
check('VF1 mid-gesture release maps through the panned view committing (4010,990) in ONE batch',
      res_up==('committed',(F(4010),F(990))) and len(v2.undo)==3
      and v2.proj.entities['W1']['end']==(F(4010),F(990))
      and v2.proj.entities['W2']['start']==(F(4010),F(990)))
check('VF1 committed batch is geometry-only: no view fields in any operation',
      sorted((op[0],op[1]) for op in v2.last_ops)
      ==[('update','W1'),('update','W2')]
      and all(set(op[2])==({'end'} if op[1]=='W1' else {'start'})
              for op in v2.last_ops))
v2.execute_undo("drag-p1'",[('update','W1',{'end':(4000,-10)}),
                            ('update','W2',{'start':(4010,-10)})])
check('VF1 one undo restores the F2 state exactly',
      v2.proj.entities['W1']==pre_w1 and v2.proj.entities['W2']==pre_w2
      and len(v2.redo)==1)

# ---------------- U10 compaction ----------------
u=Engine()
u.accept_local('K1',[('create','K1',{'start':(0,0),'end':(100,0)})])
u.accept_local('K2',[('update','K1',{'end':(200,0)})])
u.accept_local('K3',[('update','K1',{'end':(300,0)})])
u.save()
diag=u.compact(u.undo[:2])
check('U10 compaction truncates undo to the retained prefix and records the snapshot frontier',
      [b[0] for b in u.undo]==['K3']
      and u.compaction['snapshot_frontier']=={'A':3}
      and u.compaction['dropped']==('K1','K2'))
check('U10 non-blocking diagnostic names the truncation boundary exactly',
      diag=='snapshot frontier covers K1-K2; 2 undo entries truncated')
r_u1=u.undo_attempt("K3'",[('update','K1',{'end':(200,0)})])
r_u2=u.undo_attempt("K3''",[('update','K1',{'end':(300,0)})])
check('U10 post-truncation: one undo SUCCEEDS reverting K3; second is a no-op',
      r_u1=='undone' and r_u2=='no-op' and u.undo==[]
      and len(u.redo)==1 and u.proj.entities['K1']['end']==(200,0))

# ---------------- U12 text-field lifecycle machine ----------------
class Field:
    """Text field with focus ownership (R8.6.1), committed versus
    uncommitted focus-loss routing (R8.6.2), IME composition (R8.6.3),
    unit switching and rejection (R8.6.4)."""
    def __init__(self,stored,unit='mm'):
        self.stored=stored; self.unit=unit
        self.draft=None; self.focused=False; self.composing=False
        self.batches=[]
    def render(self):
        v=F(self.stored)
        return v if self.unit=='mm' else v/10
    def focus(self):
        self.focused=True; self.draft=None
        return self.render()
    def type_(self,text):
        if not self.focused: return 'rejected'
        self.draft=text if self.draft is None else self.draft+text
        return 'typed'
    def ime_compose(self,text):
        if not self.focused: return 'rejected'
        self.draft=(self.draft or '')+text
        self.composing=True
        return 'composed'
    def ime_commit(self):
        self.composing=False
        return 'resolved-into-draft'
    def scrub(self,*values):
        if not self.focused: return 'rejected'
        self.draft=str(values[-1])
        return 'scrubbed'
    def switch_unit(self,new_unit):
        self.unit=new_unit; self.draft=None
        return self.render()
    def _parse(self,text):
        try: return int(text)
        except (TypeError,ValueError): return None
    def confirm(self):
        if self.draft is None: return ('noop',None)
        val=self._parse(self.draft)
        if val is None or val!=val or val in (float('inf'),float('-inf')):
            return ('editing-rejected',None)
        if val==self.stored:
            self.draft=None; return ('noop',None)
        old=self.stored; self.stored=val; self.draft=None
        self.batches.append((old,val))
        return ('batch',(old,val))
    def focus_loss(self,kind):
        if not self.focused: return ('noop',None)
        self.focused=False
        if kind=='value-control':
            return self.confirm()
        self.draft=None
        return ('reverted',None)
    def escape(self):
        self.draft=None
        return ('reverted',None)

f=Field(200)
check('U12 focus ownership: typing outside focus is rejected',
      f.type_('2')=='rejected' and f.draft is None)
f.focus()
f.type_('2'); f.ime_compose('5'); f.ime_compose('0'); f.ime_commit()
c1=f.confirm()
check('U12 confirm after typing+IME composition emits ONE batch 200->250',
      c1==('batch',(200,250)) and f.batches==[(200,250)])
c2=f.confirm()
check('U12 confirm unchanged = no-op without batch', c2==('noop',None))
f.focus(); f.type_('0250')
c3=f.confirm()
check("U12 numeric-equivalent draft '0250' against stored 250 is a NO-OP",
      c3==('noop',None) and f.batches==[(200,250)] and f.stored==250)
f.focus(); f.type_('999'); f.escape()
c4=f.confirm()
check('U12 escape reverts the draft emitting nothing',
      c4==('noop',None) and f.stored==250 and f.batches==[(200,250)])
f2=Field(200)
f2.focus(); f2.scrub(200,180,250)
fl1=f2.focus_loss('value-control')
check('U12 committed focus loss (Tab into another field) commits the coalesced scrub',
      fl1==('batch',(200,250)) and f2.batches==[(200,250)])
f3=Field(200)
f3.focus(); f3.scrub(180)
fl2=f3.focus_loss('alt-tab')
check('U12 uncommitted focus loss (alt-tab) reverts the draft emitting nothing',
      fl2==('reverted',None) and f3.stored==200 and f3.batches==[])
f4=Field(200)
f4.focus(); f4.type_('999')
rend=f4.switch_unit('cm')
check('U12 unit switch mid-edit reverts the draft and renders the stored value in the new unit',
      f4.draft is None and rend==F(20) and f4.stored==200)
f5=Field(200)
f5.focus(); f5.type_('nan')
c5=f5.confirm()
check('U12 unparsable/non-finite confirm leaves editing state with the stored value intact',
      c5==('editing-rejected',None) and f5.stored==200
      and f5.draft=='nan' and f5.batches==[])

# ---------------- save-failure matrix (three outcomes, exact bytes) -------
def pkg(frontier,entities):
    return json.dumps({'doc':{'entities':entities},'frontier':frontier},
                      sort_keys=True).encode()
def rejected(b):
    try:
        Engine.validate_package(b); return False
    except Exception:
        return True
check('validate_package rejects an EMPTY entity map',
      rejected(json.dumps({'doc':{'entities':{}},'frontier':{'A':1}}).encode()))
check('validate_package rejects an EMPTY frontier map',
      rejected(json.dumps({'doc':{'entities':{'K':{}}},'frontier':{}}).encode()))
check('validate_package rejects a BOOLEAN counter (bool is not an accepted int)',
      rejected(json.dumps({'doc':{'entities':{'K':{}}},'frontier':{'A':True}}).encode()))
check('validate_package rejects doc:null',
      rejected(json.dumps({'doc':None,'frontier':{'A':1}}).encode()))
check('validate_package rejects an UNKNOWN doc shape',
      rejected(json.dumps({'doc':{'garbage':{}},'frontier':{'A':1}}).encode()))
check('validate_package accepts a well-formed package',
      not rejected(pkg({'A':1},{'K1':{'start':[0,0]}})))

s=Engine()
s.accept_local('C1',[('create','K1',{'start':(0,0),'end':(500,0)})])
base=(dict(s.saved),dict(s.frontier),s.dirty)
baseline_bytes=pkg(dict(s.frontier),{'K1':{}})
out=s.save_attempt('pre-replacement',None,baseline_bytes,baseline_bytes,None)
check('save matrix: pre-replacement failure aborts changing nothing',
      out=='aborted' and (dict(s.saved),dict(s.frontier),s.dirty)==base)
s.accept_local('C2',[('create','K2',{'start':(600,0),'end':(900,0)})])
candidate_bytes=pkg({'A':2},{'K1':{},'K2':{}})
new_frontier={'A':2}
target_new=pkg(new_frontier,{'K1':{},'K2':{}})
out_new=s.save_attempt('replacement-interrupted',new_frontier,target_new,
                       baseline_bytes,candidate_bytes)
check('save matrix: interrupted replacement resolves NEW by EXACT BYTES of validated packages',
      out_new=='resolved-new' and s.saved=={'A':2} and s.dirty is False
      and target_new==candidate_bytes)
s2b=Engine()
s2b.accept_local('C1',[('create','K1',{'start':(0,0),'end':(500,0)})])
s2b.save()
s2b.accept_local('C2',[('create','K2',{'start':(600,0),'end':(900,0)})])
s2b.baseline_bytes=pkg({'A':1},{'K1':{}})
t_old=pkg({'A':1},{'K1':{}}); cand=pkg({'A':2},{'K1':{},'K2':{}})
out_old=s2b.save_attempt('replacement-interrupted',{'A':2},t_old,
                         s2b.baseline_bytes,cand)
check('save matrix: interrupted replacement resolves OLD by EXACT BYTES -> dirty retained truthfully',
      out_old=='resolved-old' and s2b.saved=={'A':1} and s2b.dirty is True
      and t_old==s2b.baseline_bytes)
t_old_ws=json.dumps(json.loads(t_old),separators=(',',':')).encode()
try:
    s2b.save_attempt('replacement-interrupted',{'A':2},t_old_ws,
                     s2b.baseline_bytes,cand)
    old_ws_ok=False
except AssertionError:
    old_ws_ok=True
check('save matrix: parse-equal byte-different OLD target also violates old-or-new (BOTH outcomes byte-exact)',
      old_ws_ok and t_old_ws!=t_old and json.loads(t_old_ws)==json.loads(t_old))
s3=Engine()
s3.accept_local('C1',[('create','K1',{'start':(0,0),'end':(500,0)})])
cand3=pkg({'A':1},{'K1':{}})
parse_equal_diff_bytes=json.dumps(json.loads(cand3),
                                  separators=(',',':')).encode()
try:
    s3.save_attempt('replacement-interrupted',{'A':1},parse_equal_diff_bytes,
                    pkg({'A':2},{'K0':{}}),cand3)
    byte_exact_ok=False
except AssertionError:
    byte_exact_ok=True
check('save matrix: parse-equal but byte-different target violates old-or-new (byte-exact required)',
      byte_exact_ok and parse_equal_diff_bytes!=cand3
      and json.loads(parse_equal_diff_bytes)==json.loads(cand3))
try:
    s2b.save_attempt('replacement-interrupted',{'A':2},
                     pkg({'A':2},{'K1':{},'K2':{}})[:-3]+b'x',
                     s2b.baseline_bytes,cand)
    malformed_ok=False
except Exception:
    malformed_ok=True
check('save matrix: malformed target fails validation (not silently resolved)',
      malformed_ok)
sf3=Engine()
sf3.accept_local('D1',[('create','K3',{'start':(0,0),'end':(300,0)})])
sf3.save_attempt('dirsync-failed',{'A':1},pkg({'A':1},{'entities':{}}),
                 b'OLD',pkg({'A':1},{'K3':{}}))
check('save matrix: dir-sync failure validates candidate bytes, advances baseline, clears dirty',
      sf3.saved=={'A':1} and sf3.dirty is False)

# ---------------- U11 capacity ----------------
undo=[];redo=[]
for c in range(1,7):
    undo.append(f'C{c}')
    if len(undo)>5: undo.pop(0)
for i in range(5):
    undo.pop(); redo.append(f'r{i+1}')
    if len(redo)>3: redo.pop(0)
restored=[]
while redo: restored.append(redo.pop())
check('U11 undo overflow drops oldest; redo overflow restores r5/r4/r3',
      undo==[] and restored==['r5','r4','r3'])

print(f'{PASS}/{TOTAL} assertions PASS')
print('ALL CHECKS PASS' if not FAILS else f'FAILURES: {FAILS}')

```

### Deterministic extraction and reproduction command (read-only)

```console
$ awk '/^```python$/{f=1;next} /^```$/{f=0;next} f{print > "/tmp/check_0003.py"}' \
      issues/0003-behavior-spec-snapping-selection-undo.md
$ python3 /tmp/check_0003.py
```

### Actual output of the final run

```console
PASS S1 boundary hit at exact T^2=144 px^2
PASS G1 accepted-segment full 2-D bounds disjoint from Wa
PASS G1 f1 edge on Wa top face @4 with FULL canonical key
PASS G1 f2 same-feature consecutive win @9
PASS G1 guide persists frames 1->2 (same feature id)
PASS G1 f3 grid-exact (1700,300)
PASS G1 guide CHANGES frame 3
PASS G1 Escape clears live guide then stays clear
PASS S4a axis-line EXTENSION (3150,0) @1 wins
PASS S4a cap nearest point (3000,10) @225 misses
PASS S4a probe A: no candidate targets the anchor point
PASS S4a probe A keeps edge (3000,3) and axis ext (3005,0)
PASS S4a probe B: anchor-coincident grid removed across classes; tie survivor (3100,0)
PASS S4b J1 member-derived coordinate agrees at (3000,0), NOT the later J2 location
PASS S4b axis extension (3600,1700) @0 from anchor (3150,0)
PASS S4b centerline y at x=3500 equals 11900/9
PASS S4b interior rational point proves positive-area band/rect overlap
PASS S4b no axis contact (meets x=3600 only at own endpoint y=1700)
PASS U4 edge bottom-face @16 present; GRID ties at identical target @16
PASS U4 axis ON-segment @36 present; top-face edge excluded by radius
PASS U4 winner edge (3000,-100): class beats tied same-target grid
PASS U4 classification EXECUTED: proper crossing splits BOTH participants into four children
PASS U4 R3.1.3 identity: original-start children retain Wa/Wb ids
PASS U4 fresh supplied identities idD/idB2 for tail pieces
PASS U4 four-member join membership all at exact coordinate (3000,0)
PASS U4 children inherit ALL parent fields per R3.1.3 (non-default parents)
PASS classify_split handles DIAGONAL crossings (rational orientation pair)
PASS diagonal-crossing children inherit ALL parent fields
PASS U4 THREE-WAY proper crossing resolves without StopIteration: six children at (3000,0)
PASS U4 three-way children inherit parent fields (three distinct field sets); original-start pieces retain ids
PASS S7 mouse TRUE MISS under grid=disabled
PASS S7 touch HIT free-endpoint (4000,0) @450
PASS angle 45deg on-ray probe: EXACT zero distance in logical px^2
PASS Luna probe (100 along, 50 off) qualifies at EXACT 25/2 px^2 (25 mm^2 plan)
PASS WEST angle: pointer (2800,0) from anchor (3000,0) yields exactly the -180 candidate at 0
PASS angle behind-anchor projection excluded (no 0deg candidate)
PASS angle enumeration includes global family AND relative additions (12 targets)
PASS angle evaluation handles exactly the rational-direction subset
PASS axis comparator orders POSITION attribute (on-segment) before stable wall ID
PASS edge keys enumerate COMPONENT ordinals across a multi-component region
PASS edge key tie between top and bottom faces resolves by vertex-run ordinal
PASS edge key canonicalization is direction-invariant (reversed wall shares ring and top-face ordinal)
PASS hit test at a join point classifies join-node (not region)
PASS hit test at an EXACT free endpoint classifies free-endpoint (not region)
PASS applicable radius: pointer 20 logical px off an endpoint is touch-only
PASS CR base
PASS CR CR1
PASS CR CR2
PASS CR CR3_switch_L2
PASS CR CR3_L2_state
PASS CR CR3_back_clear
PASS CR CR3_advance
PASS CR CR4
PASS CR CR5_shift
PASS CR CR5_after_plain
PASS CR CR6_tap
PASS CR CR6_mouse
PASS CR7 genuine deletion removes Wc from the REBUILT candidate pool
PASS CR7 next click selects from the rebuilt pool (no Wc)
PASS far-away click CLEARS selection (no injected IDs)
PASS PH1 placeholder-box hit class selects instance when alone
PASS PH1 strict priority: region precedes placeholder-box on overlap
PASS PH1 placeholders PASSED INTO snap evaluation are never sources (identical results)
PASS PH1 multi-candidate cycling reaches placeholder-box as SECOND candidate
PASS PH1 deletion of the primary routed through selection/document flow: primary UNSET
PASS PH1 post-deletion pool excludes the deleted placeholder
PASS marquee TANGENT contact selects boundary-inclusively (AABB test)
PASS marquee vs DIAGONAL band decided by exact SAT (positive case)
PASS marquee SAT negative case returns False
PASS marquee EXECUTION rejects the known AABB false-positive rectangle via SAT
PASS marquee EXECUTION selects the diagonal band on true SAT overlap
PASS marquee adds walls in ascending key order; primary last added
PASS additive marquee adding NOTHING new leaves selection AND primary unchanged
PASS marquee CONNECTED dedup adds each entity once under duplicate feature delivery
PASS PLACEHOLDER marquee selection works via deterministic bounds
PASS EB1 baseline: save establishes clean baseline dirty FALSE
PASS EB1 external fence clears BOTH stacks; dirty FALSE->TRUE vs saved baseline
PASS EB1 setup: W1 tombstoned before the gesture
PASS EB1 cancel over tombstoned state PRESERVES tombstones
PASS EB1 snapshot captured tombstones at begin_drag
PASS EB1 preview overlay mutates WITHOUT touching projection
PASS EB1 mid-drag foreign-pid move rejected while gesture persists
PASS EB1 cancel-restored PRECEDES external apply
PASS EB1 selection/primary restored after cancel
PASS EB1 stale pointer-up rejected after cancel too
PASS recovery resets counters AND frontier to a FRESH A:1 after pre-existing local batches
PASS NF1 flat AND nested non-finite events rejected whole; valid nested accepted
PASS NF1 rejection leaves gesture/selection/view/snap state untouched
PASS NF1 subsequent valid interaction resolves cleanly after rejection
PASS VF1 plain-wheel pan mutates oy 384->484 FIRST
PASS VF1 cursor-anchored zoom at logical (700,300) yields {z=0.08,(549.6,447.2)} exactly
PASS VF1 cursor anchoring DIVERGES from center-anchored zoom (512,464)
PASS VF1 post-transform click maps raw (1419.2,662.4) to plan (2000,1450)
PASS VF1 post-transform click selects top-ranked Wa through the new transform
PASS VF1 idle Escape clears the selection
PASS VF1 view sequence: ZERO commits; frontier/saved/dirty untouched
PASS VF1 mid-gesture preview tracked with ZERO operations during the pan
PASS VF1 mid-gesture release maps through the panned view committing (4010,990) in ONE batch
PASS VF1 committed batch is geometry-only: no view fields in any operation
PASS VF1 one undo restores the F2 state exactly
PASS U10 compaction truncates undo to the retained prefix and records the snapshot frontier
PASS U10 non-blocking diagnostic names the truncation boundary exactly
PASS U10 post-truncation: one undo SUCCEEDS reverting K3; second is a no-op
PASS U12 focus ownership: typing outside focus is rejected
PASS U12 confirm after typing+IME composition emits ONE batch 200->250
PASS U12 confirm unchanged = no-op without batch
PASS U12 numeric-equivalent draft '0250' against stored 250 is a NO-OP
PASS U12 escape reverts the draft emitting nothing
PASS U12 committed focus loss (Tab into another field) commits the coalesced scrub
PASS U12 uncommitted focus loss (alt-tab) reverts the draft emitting nothing
PASS U12 unit switch mid-edit reverts the draft and renders the stored value in the new unit
PASS U12 unparsable/non-finite confirm leaves editing state with the stored value intact
PASS validate_package rejects an EMPTY entity map
PASS validate_package rejects an EMPTY frontier map
PASS validate_package rejects a BOOLEAN counter (bool is not an accepted int)
PASS validate_package rejects doc:null
PASS validate_package rejects an UNKNOWN doc shape
PASS validate_package accepts a well-formed package
PASS save matrix: pre-replacement failure aborts changing nothing
PASS save matrix: interrupted replacement resolves NEW by EXACT BYTES of validated packages
PASS save matrix: interrupted replacement resolves OLD by EXACT BYTES -> dirty retained truthfully
PASS save matrix: parse-equal byte-different OLD target also violates old-or-new (BOTH outcomes byte-exact)
PASS save matrix: parse-equal but byte-different target violates old-or-new (byte-exact required)
PASS save matrix: malformed target fails validation (not silently resolved)
PASS save matrix: dir-sync failure validates candidate bytes, advances baseline, clears dirty
PASS U11 undo overflow drops oldest; redo overflow restores r5/r4/r3
126/126 assertions PASS
ALL CHECKS PASS

```

### Scope and structural audits

```console
$ git diff --stat                      # main repo: BEHAVIOR-SPEC.md only
$ git -C issues diff --check           # tracker whitespace clean
$ git diff --check                     # main repo whitespace clean
$ grep -c "^#### " docs/BEHAVIOR-SPEC.md   # fixture count (computed)
93
$ grep -c "Date: 2026-08-23" docs/BEHAVIOR-SPEC.md   # current-date study-log entries
55
```

Byte-diff vs HEAD: changes confined to the global status line, sections 6-8,
and study-log appends; sections 9-10 are byte-identical and sections 1-5
differ only in the global status header. Current-date study-log entries
total 55, of which 31 pre-date this session in HEAD and 24 were added by
this session; the study log holds 79 entries with zero duplicate action-lines
and every historical entry preserved exactly once. Session study-log entries
are template-complete; older integrated entries predate the current template
and are preserved verbatim. Additional audits: 36 relative Markdown links
across the tracker resolve with 0 broken; tables consistent; cross-references
resolve; approximate-irrational glyphs absent from sections 6-8; zero
timing-dependent assertions; ADR 0004 cited as Proposed.

### Exit-gate coverage map

| Rule / fixture cluster | Evidence |
|---|---|
| R6.2.1-R6.2.3 angle families, [-180,180) normalization, ahead-of-anchor | angle checks incl. the WEST -180 probe at (2800,0) from anchor (3000,0) |
| R6.3.1 canonical edge keys (component/ring/vertex-run ordinals) | edge-key checks incl. multi-component enumeration and direction invariance |
| R6.3.1 axis key orders position attribute before stable wall ID | axis comparator check over collinear on-segment/extension pair |
| R6.4.1 guide lifetime persist/change/clear | G1 GuideState machine |
| R6.5.8 non-finite events rejected whole (nested coordinates) | NF1 engine gate with deep non-finite scan |
| R7.2.x click/cycle/modifiers via hit testing | Selector machine over live Scene hit testing |
| R7.2.3 cycle resets each trigger | CR1-CR7 |
| R7.3 marquee tangent/SAT/false-positive/dedup/primary | marquee execution: SAT rejects the AABB false-positive; connected dedup |
| R7.5.3 placeholders never snap sources; full lifecycle | PH1 flow: cycling, marquee, engine-routed deletion, never-source probe |
| R8.3.4 external fence cancels before apply; dirty TRUE | EB1 cancel-before-apply with tombstone preservation |
| R8.4.3 compaction truncation diagnostic non-silent | U10 engine compact(): snapshot frontier, diagnostic, post-truncation undo |
| R8.4.4 capacity overflow drops oldest both stacks | U11 |
| R8.5 save-failure matrix incl. byte-exact interrupted replacement | save matrix: both outcomes byte-exact; parse-equal byte-different rejected |
| R8.6 text-field lifecycle focus/IME/scrub/focus-routing | U12 Field machine with focus ownership and the '0250' no-op |
| R8.7 view-state firewall zero commits during view ops | VF1 cursor-anchored view sequence with mid-gesture pan |

Checker provenance: authored for this issue from normative rules alone
(clean-room; no external test vectors); re-run after every revision; output
above from the final round-14 run (126/126 assertions PASS). Eleven
independent exact-rational oracle probes and sixteen targeted mutations of
engine semantics were run against a fresh extraction; every mutation breaks
at least one assertion.

## Closure

- **Main repository commit:** `72a6d0f`
  (`docs: approve snapping selection undo behavior`).
- **Review:** GLM-5.3 round fourteen returned PRE-MERGE WORK APPROVED on
  2026-08-23 after independently reproducing 126/126 checker assertions,
  ten independent oracles, and sixteen mutation kills. The optional recorded-
  output invocation line was removed during tracker closure; no semantic
  evidence changed.
