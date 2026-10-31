//! Syntax-only expansion. No filesystem, environment, template or registry I/O.
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use quote::{format_ident, quote, quote_spanned};
use syn::{
    Attribute, Data, DeriveInput, Expr, Fields, Ident, LitInt, LitStr, Path, Token, Type, braced,
    parenthesized,
    parse::{Parse, ParseStream},
    parse_macro_input,
    spanned::Spanned,
};

#[proc_macro_derive(Component, attributes(component))]
pub fn derive_component(input: TokenStream) -> TokenStream {
    derive(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
struct Slot {
    name: Ident,
    minimum: usize,
    maximum: usize,
}
struct Contract {
    key: Option<LitStr>,
    runtime: Path,
    slots: Vec<Slot>,
    channels: Vec<Ident>,
}
fn contract(attrs: &[Attribute]) -> syn::Result<Contract> {
    let mut c = Contract {
        key: None,
        runtime: syn::parse_quote!(::berlin_publication),
        slots: vec![],
        channels: vec![],
    };
    for attr in attrs.iter().filter(|a| a.path().is_ident("component")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("key") {
                c.key = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("runtime") {
                c.runtime = meta.value()?.parse()?;
            } else if meta.path.is_ident("channels") {
                meta.parse_nested_meta(|m| {
                    c.channels.push(
                        m.path
                            .get_ident()
                            .cloned()
                            .ok_or_else(|| m.error("expected html or text"))?,
                    );
                    Ok(())
                })?;
            } else if meta.path.is_ident("slot") {
                let mut name = None;
                let mut minimum = 0;
                let mut maximum = 10_000;
                meta.parse_nested_meta(|m| {
                    if m.path.is_ident("name") {
                        let s: LitStr = m.value()?.parse()?;
                        name = Some(s.parse::<Ident>()?);
                    } else if m.path.is_ident("required") {
                        minimum = 1;
                    } else if m.path.is_ident("minimum") {
                        minimum = m.value()?.parse::<LitInt>()?.base10_parse()?;
                    } else if m.path.is_ident("maximum") {
                        maximum = m.value()?.parse::<LitInt>()?.base10_parse()?;
                    } else {
                        return Err(m.error("unknown slot setting"));
                    }
                    Ok(())
                })?;
                c.slots.push(Slot {
                    name: name.ok_or_else(|| meta.error("slot requires name"))?,
                    minimum,
                    maximum,
                });
            } else {
                return Err(meta.error("unknown component setting"));
            }
            Ok(())
        })?;
    }
    if c.channels.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "component requires explicit channels(html, text)",
        ));
    }
    Ok(c)
}
fn optional(ty: &Type) -> bool {
    matches!(ty,Type::Path(p) if p.path.segments.last().is_some_and(|s|s.ident=="Option"))
}
fn derive(input: DeriveInput) -> syn::Result<Tokens> {
    let c = contract(&input.attrs)?;
    let key = c
        .key
        .ok_or_else(|| syn::Error::new(input.span(), "component requires key"))?;
    let value = key.value();
    let valid = value
        .rsplit_once('@')
        .and_then(|(qualified, version)| {
            let (namespace, name) = qualified.split_once('.')?;
            let part = |s: &str| {
                !s.is_empty()
                    && s.bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            };
            let number = version.parse::<u32>().ok()?;
            (part(namespace) && part(name) && number > 0 && number.to_string() == version)
                .then_some(())
        })
        .is_some();
    if !valid {
        return Err(syn::Error::new(
            key.span(),
            "expected namespace.name@positive_version",
        ));
    }
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new(
            input.generics.span(),
            "component structs must declare concrete property types",
        ));
    }
    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => f.named.iter().collect::<Vec<_>>(),
            _ => {
                return Err(syn::Error::new(
                    s.fields.span(),
                    "expected named property fields",
                ));
            }
        },
        _ => return Err(syn::Error::new(input.span(), "Component requires a struct")),
    };
    let name = &input.ident;
    let runtime = &c.runtime;
    let builder = format_ident!("{}Builder", name);
    let mut names = fields
        .iter()
        .map(|f| f.ident.clone().unwrap())
        .collect::<Vec<_>>();
    names.extend(c.slots.iter().map(|s| s.name.clone()));
    names.push(format_ident!("key"));
    let mut seen = std::collections::BTreeSet::new();
    for n in &names {
        if !seen.insert(n.to_string()) {
            return Err(syn::Error::new(
                n.span(),
                "duplicate property, slot or reserved key",
            ));
        }
    }
    for field in &fields {
        let name = field.ident.as_ref().unwrap();
        if ["props", "publication", "site", "view", "slots"].contains(&name.to_string().as_str()) {
            return Err(syn::Error::new(name.span(), "reserved component property"));
        }
    }
    for slot in &c.slots {
        if slot.minimum > slot.maximum || slot.maximum > 10_000 {
            return Err(syn::Error::new(
                slot.name.span(),
                "invalid slot cardinality",
            ));
        }
    }
    let states = (0..names.len())
        .map(|i| format_ident!("__S{i}"))
        .collect::<Vec<_>>();
    let missing = names
        .iter()
        .map(|_| quote!(#runtime::__private::Missing))
        .collect::<Vec<_>>();
    let prop_schema=fields.iter().map(|f| {let n=f.ident.as_ref().unwrap();let t=&f.ty; quote_spanned!(f.span()=> (stringify!(#n).into(), #runtime::PropertySchema { kind:<#t as #runtime::ComponentProperty>::property_type(),default:<#t as #runtime::ComponentProperty>::default_property() }))});
    let props=fields.iter().map(|f| {let n=f.ident.as_ref().unwrap();let t=&f.ty;quote_spanned!(f.span()=> (stringify!(#n).into(),<#t as #runtime::ComponentProperty>::into_property(self.#n)))});
    let slots = c.slots.iter().map(|s| {
        let n = &s.name;
        let min = s.minimum;
        let max = s.maximum;
        quote!((stringify!(#n).into(),#runtime::SlotSchema{minimum:#min,maximum:#max}))
    });
    let channels = c
        .channels
        .iter()
        .map(|i| match i.to_string().as_str() {
            "html" => Ok(quote!(#runtime::Channel::Html)),
            "text" => Ok(quote!(#runtime::Channel::Text)),
            _ => Err(syn::Error::new(i.span(), "expected html or text")),
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let setters=names.iter().enumerate().map(|(i,n)| {
        let remaining=states.iter().enumerate().filter(|(j,_)|*j!=i).map(|(_,s)|s).collect::<Vec<_>>();
        let before=states.iter().enumerate().map(|(j,s)|if i==j {quote!(#runtime::__private::Missing)}else{quote!(#s)});
        let after=states.iter().enumerate().map(|(j,s)|if i==j {quote!(#runtime::__private::Set)}else{quote!(#s)});
        let (ty,assignment)=if i<fields.len() {let t=&fields[i].ty;
            if matches!(t,Type::Path(p) if p.path.is_ident("String")) {(quote!(impl Into<String>),quote!(self.call.properties.insert(stringify!(#n).into(), #runtime::Property::Text(value.into()));))}
            else {(quote!(#t),quote!(self.call.properties.insert(stringify!(#n).into(),<#t as #runtime::ComponentProperty>::into_property(value));))}
        } else if i<fields.len()+c.slots.len() {(quote!(impl Into<#runtime::ViewFragment>),quote!(self.call.slots.insert(stringify!(#n).into(),value.into());))}
        else {(quote!(impl Into<String>),quote!(self.call.key=Some(value.into());))};
        quote! {impl<#(#remaining),*> #builder<#(#before),*> {pub fn #n(mut self,value:#ty)->#builder<#(#after),*> {#assignment #builder{call:self.call,state: ::core::marker::PhantomData}}}}
    });
    let required = fields
        .iter()
        .map(|f| !optional(&f.ty))
        .chain(c.slots.iter().map(|s| s.minimum > 0))
        .chain([false])
        .collect::<Vec<_>>();
    let open = states
        .iter()
        .zip(&required)
        .filter(|(_, r)| !**r)
        .map(|(s, _)| s);
    let built = states.iter().zip(&required).map(|(s, r)| {
        if *r {
            quote!(#runtime::__private::Set)
        } else {
            quote!(#s)
        }
    });
    Ok(quote! {
        impl #runtime::Component for #name {
            fn schema()->#runtime::ComponentSchema {#runtime::ComponentSchema{key:#key.parse().expect("derived component key"),properties: ::std::collections::BTreeMap::from([#(#prop_schema),*]),slots: ::std::collections::BTreeMap::from([#(#slots),*]),channels:[#(#channels),*].into_iter().collect()}}
            fn properties(self)->::std::collections::BTreeMap<String,#runtime::Property> {::std::collections::BTreeMap::from([#(#props),*])}
        }
        pub struct #builder<#(#states),*> {call:#runtime::ComponentCall,state: ::core::marker::PhantomData<(#(#states,)*)>}
        impl #name {pub fn builder()->#builder<#(#missing),*> {
            let schema=<Self as #runtime::Component>::schema();
            let mut call=#runtime::ComponentCall::new(schema.key);
            for (name,property) in schema.properties {if let Some(value)=property.default {call.properties.insert(name,value);}}
            #builder{call,state: ::core::marker::PhantomData}
        }}
        #(#setters)*
        impl<#(#open),*> #builder<#(#built),*> {pub fn build(self)->#runtime::ComponentCall {self.call}}
    })
}

struct View {
    runtime: Tokens,
    nodes: Vec<Node>,
}
enum Node {
    Component(Path, Vec<(Ident, Expr)>, Vec<(Ident, Vec<Node>)>),
    Fragment(Expr),
    For(syn::Pat, Expr, Vec<Node>),
    If(Expr, Vec<Node>, Vec<Node>),
}
impl Parse for View {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut runtime = Tokens::new();
        while !input.peek(Token![;]) {
            let token: proc_macro2::TokenTree = input.parse()?;
            runtime.extend([token]);
        }
        input.parse::<Token![;]>()?;
        Ok(Self {
            runtime,
            nodes: nodes(input)?,
        })
    }
}
fn nodes(input: ParseStream) -> syn::Result<Vec<Node>> {
    let mut result = vec![];
    while !input.is_empty() {
        if input.peek(Token![for]) {
            input.parse::<Token![for]>()?;
            let pat = syn::Pat::parse_multi_with_leading_vert(input)?;
            input.parse::<Token![in]>()?;
            let expr = Expr::parse_without_eager_brace(input)?;
            let body;
            braced!(body in input);
            result.push(Node::For(pat, expr, nodes(&body)?));
        } else if input.peek(Token![if]) {
            input.parse::<Token![if]>()?;
            let expr = Expr::parse_without_eager_brace(input)?;
            let body;
            braced!(body in input);
            let yes = nodes(&body)?;
            let no = if input.peek(Token![else]) {
                input.parse::<Token![else]>()?;
                let other;
                braced!(other in input);
                nodes(&other)?
            } else {
                vec![]
            };
            result.push(Node::If(expr, yes, no));
        } else if input.peek(syn::token::Paren) {
            let expr;
            parenthesized!(expr in input);
            result.push(Node::Fragment(expr.parse()?));
        } else {
            let path: Path = input.parse()?;
            let args;
            parenthesized!(args in input);
            let mut props = vec![];
            while !args.is_empty() {
                let name: Ident = args.parse()?;
                args.parse::<Token![:]>()?;
                props.push((name, args.parse()?));
                if args.is_empty() {
                    break;
                }
                args.parse::<Token![,]>()?;
            }
            let mut slots = vec![];
            if input.peek(syn::token::Brace) {
                let body;
                braced!(body in input);
                let lookahead = body.fork();
                let named_slot = if lookahead.peek(Ident)
                    && lookahead.parse::<Ident>()? == "slot"
                    && lookahead.peek(Ident)
                {
                    lookahead.parse::<Ident>()?;
                    lookahead.peek(syn::token::Brace)
                } else {
                    false
                };
                if named_slot {
                    while !body.is_empty() {
                        let slot: Ident = body.parse()?;
                        if slot != "slot" {
                            return Err(syn::Error::new(slot.span(), "expected named slot"));
                        }
                        let name: Ident = body.parse()?;
                        let content;
                        braced!(content in body);
                        slots.push((name, nodes(&content)?));
                    }
                } else {
                    slots.push((Ident::new("body", Span::call_site()), nodes(&body)?));
                }
            }
            result.push(Node::Component(path, props, slots));
        }
    }
    Ok(result)
}
fn emit(nodes: &[Node], runtime: &Tokens) -> Tokens {
    let output = Ident::new("__berlin_fragment", Span::mixed_site());
    let pieces = nodes.iter().map(|node| match node {
        Node::Fragment(expr) => quote!(#output.0.extend(#runtime::ViewFragment::from(#expr).0);),
        Node::For(pat, expr, body) => {
            let body = emit(body, runtime);
            quote!(for #pat in #expr {#output.0.extend((#body).0);})
        }
        Node::If(expr, yes, no) => {
            let yes = emit(yes, runtime);
            let no = emit(no, runtime);
            quote!(if #expr {#output.0.extend((#yes).0);}else{#output.0.extend((#no).0);})
        }
        Node::Component(path, props, slots) => {
            let setters = props.iter().map(|(name, expr)| quote!(.#name(#expr)));
            let slots = slots.iter().map(|(name, children)| {
                let children = emit(children, runtime);
                quote!(.#name(#children))
            });
            quote!(#output.0.push(#path::builder()#(#setters)*#(#slots)*.build());)
        }
    });
    quote!({let mut #output=#runtime::ViewFragment::default();#(#pieces)* #output})
}
#[proc_macro]
pub fn __view(input: TokenStream) -> TokenStream {
    let v = parse_macro_input!(input as View);
    emit(&v.nodes, &v.runtime).into()
}
