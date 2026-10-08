#!/usr/bin/env python3
import argparse, json, collections
from pathlib import Path
parser=argparse.ArgumentParser(description="Partition an Id activation tree into disjoint causal work categories.")
parser.add_argument("input",type=Path);parser.add_argument("--output",type=Path,required=True)
args=parser.parse_args()
p=json.loads(args.input.read_text())
m=p['raw']['cases'][0]['work_counts']['candidate']['steady_messages'];trace=m['trace']
operations=m['operations']
assert operations>0
assert not p['inputs_changed_during_run']
by={r['id']:r for r in trace};children=collections.defaultdict(list)
for r in trace:
 assert r['parent']<r['id']
 children[r['parent']].append(r)
 assert r['inclusive']>=1
 assert r['id']+r['inclusive']<=len(trace)+1
assert collections.Counter(r['selector'] for r in trace)==m['selectors']
for r in trace:
 assert 1+sum(c['inclusive'] for c in children[r['id']])==r['inclusive'],r
seeds={}; reasons={}
constructors_selectors={'fromMethod:codeObject:referenceSlots:','fromMethod:codeObject:captureOperands:'}
construct={'fromMethod:codeObject:captureOperands:','allocateReferenceSlots:','slotAt:in:put:','fromMethod:codeObject:referenceSlots:'}
abi={'discardResultOf:selector:wordArgument:','objectResultOf:selector:byteArgument:','bytesResultOf:selector:resultFactory:','optionalWordResultOf:selector:argument:resultFactory:','objectResultOf:selector:wordArgument:','tryNativeArguments:','sendNativeTo:selector:arguments:signature:resultFactory:','wordResultOf:selector:','bytesResultOf:selector:'}
word={'tryPrimitiveExpression:','tryPrimitiveExpressionAtEpoch:','wrappingAdd:','bitXor:','bitAnd:','unsignedLessThan:'}
controlwords={'selectIfSameObject:with:ifEqual:ifDifferent:','sameObject:with:','selectIfZero:ifNonzero:'}
root={'retain:','release:','tryRetainMany:','tryReleaseMany:','tryReleaseManyForward:'}
for r in trace:
 sel=r['selector']
 cat='closure construction' if sel in construct else 'capture access' if sel in {'captureWord:of:','captureWordAt:','captureReadModeOf:'} else 'ABI bridges' if sel in abi else 'Boolean selection primitives' if sel in controlwords else 'Word arithmetic/guards' if sel in word else None
 if cat:seeds[r['id']]=cat;reasons[r['id']]='semantic operation '+sel
# Namespace factory lookups are preparation for the following generated owner call.
# Skip lifetime operations when finding that call; retain those preparation roots later.
for rows in children.values():
 for i,r in enumerate(rows):
  if r['selector']!='at:' or r['key'] not in {'Block','ObjectModel'}:continue
  nextop=next((q for q in rows[i+1:] if q['selector'] not in root),None)
  if nextop and nextop['selector'] in construct:
   seeds[r['id']]='closure construction';reasons[r['id']]='adjacent factory lookup '+r['key']+' for '+nextop['selector']
  elif nextop and r['key']=='Block' and nextop['selector'] in {'captureWord:of:','captureWordAt:','captureReadModeOf:'}:
   seeds[r['id']]='capture access';reasons[r['id']]='adjacent Block factory lookup for capture projection'
# A generated retain of a sibling operation's exact returned Object is attributed
# to that producer. Addresses are opaque Id references; no payload is read.
for rows in children.values():
 last={}
 for r in rows:
  if r['selector']=='retain:' and r['argument']!='0' and r['argument'] in last:
   producer=last[r['argument']]
   if producer['id'] in seeds:
    seeds[r['id']]=seeds[producer['id']];reasons[r['id']]='retain of returned Object from '+str(producer['id'])
  if r['result']!='0' and r['selector'] not in root:last[r['result']]=r
# Outer structural cause wins. This partitions nested root/name/check costs by
# what requested them, instead of adding overlapping inclusive call trees.
category={};counts=collections.Counter();direct=collections.Counter();cut=[]
for r in trace:
 parentcat=category.get(r['parent'])
 if parentcat is not None:
  category[r['id']]=parentcat
 elif r['id'] in seeds:
  category[r['id']]=seeds[r['id']]
  cut.append({'id':r['id'],'selector':r['selector'],'key':r['key'],'category':seeds[r['id']],'sends':r['inclusive'],'reason':reasons[r['id']]})
 else:category[r['id']]=None
# Classify remaining work at its own operation, not as a parent envelope that
# would swallow distinct construction in selected branches.
validation={'integerExactError:','valueAt:','isNil','notNil','isError','vtableOf:','==','&','at:'}
for r in trace:
 c=category[r['id']]
 if c is None:
  # Remaining exact root operations include their proof descendants.
  chain=[];q=r
  while q['parent']:
   chain.append(q);q=by[q['parent']]
  rooted=next((q for q in reversed(chain) if q['selector'] in root and category[q['id']] is None),None)
  checked=next((q for q in reversed(chain) if q['selector'] in validation and category[q['id']] is None),None)
  if rooted:c='other lifetime/cleanup'
  elif checked:c='validation/other binding lookup'
  elif r['selector'] in {'intern:','symbolNamed:','symbolBytesOf:'}:c='other name handling'
  else:c='dispatch/policy scaffolding'
 counts[c]+=1;direct[(c,r['selector'])]+=1
assert sum(counts.values())==len(trace)
# Exclusive envelopes: how much remains in each named application/control send
# after nested structural categories have been charged to their causes.
envelopes=[]
for sel in ['+','integerBinary:operation:','integerBitsOf:','integerExactError:','integerAddBits:with:','integerFromBits:','ifTrue:ifFalse:']:
 rows=[r for r in trace if r['selector']==sel]
 if rows:envelopes.append({'selector':sel,'calls':len(rows)/operations,'inclusive_mean':sum(r['inclusive'] for r in rows)/len(rows)})
summary={'total':len(trace),'operations':operations,'sends_per_addition':len(trace)/operations,
 'partition':[{'category':c,'count':n,'per_addition':n/operations,'percent':100*n/len(trace)} for c,n in counts.most_common()],
 'category_selectors':[{ 'category':c,'selector':s,'count':n,'per_addition':n/operations} for (c,s),n in direct.most_common()],
 'attribution_cut':cut,'application_envelopes':envelopes,
 'leaf_selectors':[{'selector':s,'per_addition':n/operations,'percent':100*n/len(trace)} for s,n in sorted(m['selectors'].items(),key=lambda x:-x[1])],
 'scope':'Receipt worktree and library hashes, successful positive Integer chain, untimed after warmup; grouping is causal classification, not CPU share or proof of owner semantics.'}
# Associate construction preparations with the subsequent constructor in the
# same generated-body activation. This identifies allocated-but-unused branches.
constructors=[r for r in trace if r['selector'] in constructors_selectors]
called=collections.Counter(r['receiver'] for r in trace if r['selector']=='value' or (r['selector'].startswith('value:') and r['selector'].replace('value:','')==''))
closure_seeds={r['id']:r for r in cut if r['category']=='closure construction'}
groups=[]
for rows in children.values():
 pending=[]
 for r in rows:
  if r['id'] in closure_seeds:pending.append(closure_seeds[r['id']])
  if r['selector'] in constructors_selectors:
   after=[q for q in cut if q['category']=='closure construction' and q['reason']=='retain of returned Object from '+str(r['id'])]
   groups.append({'ctor_id':r['id'],'block':r['result'],'invoked':bool(called[r['result']]),
     'sends':sum(q['sends'] for q in pending)+sum(q['sends'] for q in after),
     'capture_count':sum(q['selector']=='slotAt:in:put:' for q in pending) if r['selector']=='fromMethod:codeObject:referenceSlots:' else None,
     'cut_ids':[q['id'] for q in pending]+[q['id'] for q in after]})
   pending=[]
  if pending and pending[-1]['reason'].startswith('retain of returned Object from '):
   producer=int(pending[-1]['reason'].split()[-1])
   if by[producer]['selector'] in constructors_selectors:pending.pop()
assert sum(g['sends'] for g in groups)==counts['closure construction']
assert len(groups)==len(constructors)
exports=[r for r in trace if r['selector']=='canonicalBytesOf:']
summary['closure_groups']=groups
summary['closure_summary']={'constructed_per_addition':len(groups)/operations,
 'invoked_per_addition':sum(g['invoked'] for g in groups)/operations,
 'unused_per_addition':sum(not g['invoked'] for g in groups)/operations,
 'unused_construction_sends_per_addition':sum(g['sends'] for g in groups if not g['invoked'])/operations,
 'canonical_export_calls_per_addition':len(exports)/operations,
 'canonical_export_sends_per_addition':sum(r['inclusive'] for r in exports)/operations}
summary['construction_cut']=[{'selector':s,'sends_per_addition':sum(r['sends'] for r in cut if r['category']=='closure construction' and r['selector']==s)/operations} for s in ['fromMethod:codeObject:referenceSlots:','fromMethod:codeObject:captureOperands:','allocateReferenceSlots:','slotAt:in:put:','at:','retain:']]
summary['input_sha256']=__import__('hashlib').sha256(args.input.read_bytes()).hexdigest()
args.output.parent.mkdir(parents=True,exist_ok=True)
args.output.write_text(json.dumps(summary,indent=2)+'\n')
for row in summary['partition']:print(row)
print('envelopes',envelopes)
