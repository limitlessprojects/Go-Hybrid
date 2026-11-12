//! Regression matrix for #883: match arm guards evaluate after binding pattern variables.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

const H: &str = "struct sv{n int}\nstruct bm{m $}\nconst cm=${\"k\":1}\nconst xs=[]sv{sv{n:1}}\nf\
n nm()${return ${\"k\":1}}\nfn ns()map[string]sv{return map[string]sv{\"a\":sv{n:1}}}\nfn w(m $){\
m[\"k\"]=1}\nfn t(m $)bool{return len(m)>0}\nfn b2(m $,n int)bool{return len(m)+n>0}\nfn rd(m $)b\
ool{return m[\"k\"]>0}\nfn mm(m $)bool{m[\"k\"]=9\nreturn true}\nfn pm(p *sv)bool{p.n=9\nreturn t\
rue}\nfn ins(m map[string]sv)bool{m[\"b\"]=sv{n:2}\nreturn true}\nfn mk()option[$]{return some(nm\
())}\nfn put(b []$,m $)bool{b[0]=m\nreturn true}\n";
const CASES: &str = "E3003|#m:=mk()\n%m{!t(move v):{v[\"k\"]=2}\n@}~E3003|#m:=nm()\n%some(m){!t(m\
ove v):{}\n@}\nm[\"k\"]=2~E3003|#%mk(){!t(move v):{v[\"k\"]=2}\n@}~E3003|#%mk(){!b2(move v,v[\"k\
\"]):{}\n@}~E3003|#m:=nm()\no:=some(m)\ntask w(move m)\n%o{!rd(v):{}\n@}~E3003|#%mk(){!t(move v):\
{}\nsome(u):{u[\"k\"]=2}\nnone:{}}~E3003|#m:=nm()\no:=some(m)\nfor i in 0..2{_=i\n_=m[\"k\"]\n%o{\
!t(move v):{}\n@}}~E3003|#f:=fn(){%mk(){!b2(move v,v[\"k\"]):{}\n@}}\nf()~E3004|#b:=bm{m:nm()}\n%\
some(b){!t(move v.m):{}\n@}~E3006|#%some(cm){!mm(v):{}\n@}~E3006|#%some(cm){!(fn()bool{delete(v,\
\"k\")\nreturn true})():{}\n@}~E3006|#%some(xs){some(c) if pm(&c[0]):{}\n@}~E3102|#m:=ns()\ns:=&m\
[\"a\"]\n%some(m){!ins(v):{}\n@}\n_=s.n~E3102|#f:=fn(){m:=ns()\ns:=&m[\"a\"]\n%some(m){!ins(v):{}\
\n@}\n_=s.n}\nf()~E3102|#m:=ns()\no:=some(m)\ns:=&m[\"a\"]\nf:=fn(){%o{!ins(v):{}\n@}}\nf()\n_=s.\
n~OK|#v:=42\nf:=fn(){%mk(){!t(move v):{}\n@}}\nf()\n_=v~OK|#m:=ns()\nv:=&m[\"a\"]\n%some(2){some(\
x) if x>v.n:{m[\"b\"]=sv{n:2}}\n@}\n_=m~OK|#m:=ns()\nv:=&m[\"a\"]\n_=v.n\nm[\"b\"]=sv{n:2}\n%some\
(1){!v>0:{}\n@}~OK|#%some(cm){!rd(v):{_=v[\"k\"]}\n@}~OK|#v:=ns()\ns:=&v[\"a\"]\nf:=fn(){l:=ns()\
\n%some(l){!ins(v):{}\n@}}\nf()\n_=s.n~E3004|@Hfn via(b []$,o option[$]){\n%o{!put(b,v):{}\n@}}\n\
fn run(p $){b:=[]${nm()}\nvia(b,some(p))\ntask w(move b[0])}\nfn main(){run(nm())}~E3006|@Hfn h(o \
option[$]){\n%o{!(fn()bool{v[\"k\"]=2\nreturn true})():{}\n@}}\nfn main(){h(some(cm))}~E3101|impo\
rt \"rust/serde_json\"\nfn run()error{doc:=serde_json.parse(`{}`)?\nitems:=doc.array(\"a\")?\n%so\
me(doc){some(d) if d.set(\"x\",1)==nil:{}\n@}\n_=len(items)\nreturn nil}\nfn main(){_=run()}~E300\
3|import \"rust/flate2\"\nfn run()error{enc:=flate2.new_writer(6)?\n%some(enc){some(z) if len(z.f\
inish()?)>=0:{_=z.write([]byte{1})?}\n@}\nreturn nil}\nfn main(){_=run()}~E3003|import \"rust/fla\
te2\"\nfn mkz()(option[flate2.writer],error){z:=flate2.new_writer(6)?\nreturn some(z),nil}\nfn ru\
n()error{%mkz()?{some(z) if len(z.finish()?)>99:{}\nsome(z2):{_=z2.write([]byte{1})?}\nnone:{}}\n\
return nil}\nfn main(){_=run()}";

#[test]
fn match_guard_pattern_binding_883() {
    for (i, entry) in CASES.split('~').enumerate() {
        let (want, raw) = entry.split_once('|').unwrap();
        let w = raw
            .strip_prefix('#')
            .map(|b| format!("@Hfn main(){{\n{b}\n}}"));
        let s = w
            .as_deref()
            .unwrap_or(raw)
            .replace("@H", H)
            .replace('$', "map[string]int");
        let s = s.replace('%', "match ").replace('!', "some(v) if ");
        let r = check_file(&parse_file("t.hy", &s.replace('@', "some(_):{}\nnone:{}")).unwrap());
        let c = format!("HYB-{want}");
        let ok = (want == "OK" && r.verdict == Verdict::Accept)
            || r.diagnostics.iter().any(|d| d.code == c);
        assert!(ok, "case {i}: {:?}", r.diagnostics);
    }
}
