"""Independent Python model for Logue's seeded generator and wait-only arenas.

Uses Python integers/dicts/lists, no Birnam output as golden data. Mirrors game
rules, including RNG consumption and bounded message trimming, not VM details.
"""
from copy import deepcopy

class RNG:
    def __init__(self, seed): self.state = seed % 2147483647 + 1
    def n(self, n):
        self.state = (1103515245 * self.state + 12345) % 2147483648
        return self.state % n
    def between(self, lo, hi): return lo + self.n(hi - lo + 1)

def player():
    return dict(class_='fighter', hp=30, maxHp=30, str=5, weapon='sword',
                weaponDmg=4, armor='chainmail', armorDef=3, gold=10, rations=2,
                potions=1, mana=0, maxMana=0, x=0, y=0, hunger=1000, xp=0,
                plvl=1, hasMac=False)

def hero():
    p=player(); p['class']=p.pop('class_'); return p

def monster(d, x, y, identity):
    name,glyph,hp,atk,defense,xp=[('rat',114,3,2,0,2),('goblin',103,6,3,1,4),
        ('orc',111,11,5,1,7),('ogre',79,18,8,2,12),('wraith',87,24,10,3,18)][d-1]
    return dict(name=name,glyph=glyph,hp=hp,atk=atk,**{'def':defense},xp=xp,id=identity,x=x,y=y)

def generation(seed):
    rng=RNG(seed); levels={}; identity=0
    for d in range(1,6):
        tiles={}; monsters=[]; items=[]; rooms=[]
        for _ in range(rng.between(5,7)):
            w=rng.between(5,11); h=rng.between(3,5)
            rooms.append(dict(x=rng.between(1,60-w-1),y=rng.between(1,18-h-1),w=w,h=h))
        def put(x,y,kind='floor'): tiles[str(y*60+x)]=kind
        def center(r): return [r['x']+r['w']//2,r['y']+r['h']//2]
        for r in rooms:
            for y in range(r['y'],r['y']+r['h']):
                for x in range(r['x'],r['x']+r['w']): put(x,y)
        for a,b in zip(map(center,rooms),map(center,rooms[1:])):
            for x in range(min(a[0],b[0]),max(a[0],b[0])+1): put(x,a[1])
            for y in range(min(a[1],b[1]),max(a[1],b[1])+1): put(b[0],y)
        first,last=center(rooms[0]),center(rooms[-1]); put(*first,'up')
        if d<5: put(*last,'down')
        else: items.append(dict(x=last[0],y=last[1],kind='macguffin'))
        for r in rooms[1:]:
            c=center(r)
            for _ in range(rng.between(0,2)):
                x=rng.between(r['x'],r['x']+r['w']-1); y=rng.between(r['y'],r['y']+r['h']-1)
                if [x,y]!=c and not any(m['x']==x and m['y']==y for m in monsters):
                    identity+=1; monsters.append(monster(d,x,y,identity))
            if rng.n(2)==0:
                x=rng.between(r['x'],r['x']+r['w']-1); y=rng.between(r['y'],r['y']+r['h']-1)
                if [x,y]!=c:
                    roll=rng.n(100); item=dict(x=x,y=y)
                    if roll<30: item.update(kind='gold',amount=rng.between(5,20*d))
                    elif roll<55: item.update(kind='food')
                    elif roll<72: item.update(kind='potion',heal=rng.between(8,16))
                    elif roll<87: item.update(kind='weapon',name='short sword' if d<=2 else 'war axe' if d==3 else 'great mace',dmg=2+d+rng.n(2))
                    else: item.update(kind='armor',name='ring mail' if d<=2 else 'plate mail' if d==3 else 'dragon scale',**{'def':d+rng.n(2)})
                    items.append(item)
        levels[str(d)]=dict(tiles=tiles,monsters=monsters,items=items,up=first,down=last,rooms=rooms)
    p=hero(); p['x'],p['y']=levels['1']['up']
    return dict(player=p,monsters=levels['1']['monsters'],items=levels['1']['items'],mode='play',turn=0,
                rng=rng.state,tile_count=len(levels['1']['tiles']),message_count=1,levels=levels)

class Arena:
    def __init__(self, seed, width, count):
        self.rng=RNG(seed); self.width=width; self.p=hero()
        self.p.update(x=2,y=5,hp=10000,maxHp=10000)
        self.monsters=[monster(1,width-1-i//8,i%8+1,i+1) for i in range(count)]
        self.turn=0; self.messages=0
    def step(self, frozen=False):
        if not frozen:
            # Birnam traverses the old Vector, replacing entries in the live one.
            for old in deepcopy(self.monsters):
                dx=self.p['x']-old['x']; dy=self.p['y']-old['y']
                if max(abs(dx),abs(dy))==1:
                    self.p['hp']-=max(old['atk']+self.rng.n(2)-self.p['armorDef'],1)
                    self.messages=min(6,self.messages+1)
                else:
                    dx=(dx>0)-(dx<0); dy=(dy>0)-(dy<0)
                    for sx,sy in [(dx,dy),(dx,0),(0,dy)]:
                        x,y=old['x']+sx,old['y']+sy
                        if (1<=x<=self.width and 1<=y<=10 and (x,y)!=(self.p['x'],self.p['y'])
                            and not any((m['x'],m['y'])==(x,y) for m in self.monsters)):
                            self.monsters[old['id']-1].update(x=x,y=y); break
        self.turn+=1; self.p['hunger']-=1
    def snapshot(self):
        return deepcopy(dict(player=self.p,monsters=self.monsters,items=[],mode='play',turn=self.turn,
                             rng=self.rng.state,tile_count=self.width*10,message_count=self.messages))

def wire(state):
    """Birnam JSON spells Symbol keys/values with #; preserve that distinction."""
    def symbols(row, enums=()):
        return {'#'+k: '#'+v if k in enums else v for k,v in row.items()}
    state=deepcopy(state)
    state['player']=symbols(state['player'],('class',))
    state['monsters']=[symbols(m) for m in state['monsters']]
    state['items']=[symbols(i,('kind',)) for i in state['items']]
    state['mode']='#'+state['mode']
    if 'levels' in state:
        for d,level in state['levels'].items():
            level['tiles']={k:'#'+v for k,v in level['tiles'].items()}
            level['monsters']=[symbols(m) for m in level['monsters']]
            level['items']=[symbols(i,('kind',)) for i in level['items']]
            level['rooms']=[symbols(r) for r in level['rooms']]
            state['levels'][d]=symbols(level)
    return state

def expected(config):
    samples=[]; replacement=None
    for _ in range(config['samples']):
        arena=Arena(config['seed'],config['width'],config['monsters'])
        batches=[]
        for _ in range(config['batches']):
            if config['phase']=='generation': state=generation(config['seed'])
            else:
                for _ in range(config['turns']): arena.step()
                state=arena.snapshot()
            batches.append(wire(state))
        samples.append(batches)
    if config['replace']:
        arena.step(frozen=True); replacement=wire(arena.snapshot())
    return samples,replacement
