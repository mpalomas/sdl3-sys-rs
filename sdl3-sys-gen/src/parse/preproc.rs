use super::{
    DocComment, DocCommentPost, GetSpan, Ident, IdentOrKw, Item, Items, Parse, ParseErr,
    ParseRawRes, Punctuated, Span, WsAndComments,
};
use std::borrow::Cow;

pub struct Define {
    span: Span,
    doc: Option<DocComment>,
    ident: Ident,
    args: Option<Punctuated<Ident, Op![,]>>,
    expr: Span,
}

impl GetSpan for Define {
    fn span(&self) -> Span {
        self.span.clone()
    }
}

pub struct Include {
    span: Span,
    kind: IncludeKind,
    path: Span,
}

impl GetSpan for Include {
    fn span(&self) -> Span {
        self.span.clone()
    }
}

pub enum IncludeKind {
    Local,
    System,
}

pub struct Line {
    span: Span,
}

impl Parse for Line {
    fn desc() -> Cow<'static, str> {
        "line".into()
    }

    fn parse_raw(input: &Span) -> ParseRawRes<Self> {
        let input = &input.trim_wsc_start()?;
        let mut escaped = false;
        let (rest, span) = 'parse: {
            for (i, ch) in input.char_indices() {
                if ch == '\n' && !escaped {
                    let rest = input.slice(i + 1..);
                    let span = input.slice(..i);
                    break 'parse (rest, span);
                }
                escaped = ch == '\\';
            }
            break 'parse (input.end(), input.clone());
        };
        let span = span.trim_wsc_end()?;
        Ok((rest, Line { span }))
    }
}

pub struct PreProcBlock<const ALLOW_INITIAL_ELSE: bool = false> {
    span: Span,
    kind: PreProcBlockKind,
    block: Items,
    else_block: Option<Box<PreProcBlock<true>>>,
}

impl<const ALLOW_INITIAL_ELSE: bool> GetSpan for PreProcBlock<ALLOW_INITIAL_ELSE> {
    fn span(&self) -> Span {
        self.span.clone()
    }
}

pub enum PreProcBlockKind {
    If(Span),
    IfDef(Ident),
    IfNDef(Ident),
    None,
}

impl<const ALLOW_INITIAL_ELSE: bool> Parse for PreProcBlock<ALLOW_INITIAL_ELSE> {
    fn desc() -> Cow<'static, str> {
        "preprocessor block".into()
    }

    fn try_parse_raw(input: &Span) -> ParseRawRes<Option<Self>> {
        if let (mut rest, Some(line)) = PreProcLine::try_parse_raw(input)? {
            let span0 = line.span;

            let (kind, is_else) = match line.kind {
                PreProcLineKind::If(expr) => (PreProcBlockKind::If(expr), false),
                PreProcLineKind::IfDef(ident) => (PreProcBlockKind::IfDef(ident), false),
                PreProcLineKind::IfNDef(ident) => (PreProcBlockKind::IfNDef(ident), false),

                PreProcLineKind::ElIf(expr) => (PreProcBlockKind::If(expr), true),
                PreProcLineKind::ElIfDef(ident) => (PreProcBlockKind::IfDef(ident), true),
                PreProcLineKind::ElIfNDef(ident) => (PreProcBlockKind::IfNDef(ident), true),
                PreProcLineKind::Else => (PreProcBlockKind::None, true),

                PreProcLineKind::EndIf
                | PreProcLineKind::Define(_)
                | PreProcLineKind::Include(_) => return Ok((input.clone(), None)),
            };

            if !ALLOW_INITIAL_ELSE && is_else {
                return Ok((input.clone(), None));
            }

            let block_start = rest.start();

            while !rest.is_empty() {
                // skip doc comments so the end of one doesn't break Line
                if DocComment::try_parse(&mut rest)?.is_some() {
                } else if let (rest_, Some(line)) = Line::try_parse_raw(&rest)? {
                    let span = line.span.clone();
                    let mut line = line.span;
                    if !line.as_str().contains('#') && line.as_str().contains("/**<") {
                        // skip postfix doc comment
                        let pos = rest
                            .as_str()
                            .as_bytes()
                            .windows(4)
                            .position(|b| b == b"/**<")
                            .unwrap();
                        rest = rest.slice(pos..);
                        DocCommentPost::parse(&mut rest)?;
                        continue;
                    }
                    if let Some(pp) = PreProcLine::try_parse(&mut line)? {
                        match pp.kind {
                            PreProcLineKind::If(_)
                            | PreProcLineKind::IfDef(_)
                            | PreProcLineKind::IfNDef(_) => {
                                // skip nested if block
                                PreProcBlock::<false>::parse(&mut rest)?;
                                continue;
                            }

                            PreProcLineKind::ElIf(_)
                            | PreProcLineKind::ElIfDef(_)
                            | PreProcLineKind::ElIfNDef(_)
                            | PreProcLineKind::Else => {
                                let block = block_start.join(&rest.start());
                                let block = match &kind {
                                    PreProcBlockKind::None => {
                                        return Err(ParseErr::new(
                                            pp.span,
                                            "expected `#endif` after `#else`, got another else",
                                        ))
                                    }

                                    PreProcBlockKind::IfDef(i)
                                        if i.span.as_str() == "__cplusplus" =>
                                    {
                                        vec![Item::Skipped(block)]
                                    }

                                    _ => Items::parse_all(block.trim_wsc()?)?,
                                };
                                let (rest, else_block) = PreProcBlock::<true>::parse_raw(&rest)?;
                                let span1 = else_block.span();
                                return Ok((
                                    rest,
                                    Some(Self {
                                        span: span0.start().join(&span1.end()),
                                        kind,
                                        block,
                                        else_block: Some(Box::new(else_block)),
                                    }),
                                ));
                            }

                            PreProcLineKind::EndIf => {
                                let block = block_start.join(&rest.start());
                                let block = match &kind {
                                    PreProcBlockKind::IfDef(i)
                                        if i.span.as_str() == "__cplusplus" =>
                                    {
                                        vec![Item::Skipped(block)]
                                    }

                                    _ => Items::parse_all(block.trim_wsc()?)?,
                                };
                                let rest = rest_;
                                return Ok((
                                    rest,
                                    Some(Self {
                                        span: span0.start().join(&span.end()),
                                        kind,
                                        block,
                                        else_block: None,
                                    }),
                                ));
                            }

                            PreProcLineKind::Define(_) | PreProcLineKind::Include(_) => {
                                rest = rest_;
                                continue;
                            }
                        }
                    } else {
                        rest = rest_;
                    }
                } else {
                    break;
                }
            }

            Err(ParseErr::new(span0, "unterminated #if"))
        } else {
            Ok((input.clone(), None))
        }
    }
}

pub struct PreProcLine {
    pub span: Span,
    pub kind: PreProcLineKind,
}

pub enum PreProcLineKind {
    If(Span),
    IfDef(Ident),
    IfNDef(Ident),
    ElIf(Span),
    ElIfDef(Ident),
    ElIfNDef(Ident),
    Else,
    EndIf,
    Define(Define),
    Include(Include),
}

impl Parse for PreProcLine {
    fn desc() -> Cow<'static, str> {
        "preprocessor directive".into()
    }

    fn try_parse_raw(input: &Span) -> ParseRawRes<Option<Self>> {
        let (rest, doc) = DocComment::try_parse_raw(input)?;
        if doc.is_some() && !rest.starts_with_ch('#') {
            // detached doc comment
            return Ok((input.clone(), None));
        }
        let (rest, line) = Line::parse_raw(&rest)?;
        let span = line.span;
        let line = span.trim_wsc()?;
        if let Some(i) = line.strip_prefix_ch('#') {
            if let Some(doc) = &doc {
                if !i.starts_with("define") {
                    return Err(ParseErr::new(
                        doc.span(),
                        "doc comment for preprocessor directive other than define",
                    ));
                }
            }
            let mut i = i.trim_wsc_start()?;
            let ident = IdentOrKw::parse(&mut i)
                .map_err(|e| e.map_msg("expected preprocessor directive"))?;
            WsAndComments::try_parse(&mut i)?;

            let kind = match ident.as_str() {
                "if" => PreProcLineKind::If(i),
                "ifdef" => PreProcLineKind::IfDef(Ident::parse_all(i)?),
                "ifndef" => PreProcLineKind::IfNDef(Ident::parse_all(i)?),

                "elif" => PreProcLineKind::ElIf(i),
                "elifdef" => PreProcLineKind::ElIfDef(Ident::parse_all(i)?),
                "enifndef" => PreProcLineKind::ElIfNDef(Ident::parse_all(i)?),

                "else" => PreProcLineKind::Else,
                "endif" => PreProcLineKind::EndIf,

                "define" => {
                    let ident = Ident::parse(&mut i)?;
                    if i.starts_with_ch('(') {
                        if let Some(close_paren) = i.as_bytes().iter().position(|&b| b == b')') {
                            let args = Punctuated::<Ident, Op![,]>::try_parse_all(
                                i.slice(1..close_paren).trim_wsc()?,
                            )?
                            .unwrap_or_default();
                            PreProcLineKind::Define(Define {
                                span: span.clone(),
                                doc,
                                ident,
                                args: Some(args),
                                expr: i.slice(close_paren + 1..),
                            })
                        } else {
                            return Err(ParseErr::new(i.slice(0..=0), "unmatched `(`"));
                        }
                    } else {
                        WsAndComments::try_parse(&mut i)?;
                        PreProcLineKind::Define(Define {
                            span: span.clone(),
                            doc,
                            ident,
                            args: None,
                            expr: i,
                        })
                    }
                }

                "include" => {
                    let i = i.trim_wsc_start()?;
                    let e = i.as_bytes()[i.len() - 1];
                    let kind = match i.as_bytes()[0] {
                        b'<' => {
                            if e != b'>' {
                                return Err(ParseErr::new(i.slice(i.len() - 1..), "expected `>`"));
                            }
                            IncludeKind::System
                        }
                        b'"' => {
                            if e != b'"' {
                                return Err(ParseErr::new(i.slice(i.len() - 1..), "expected `\"`"));
                            }
                            IncludeKind::Local
                        }
                        _ => return Err(ParseErr::new(i, "malformed include path")),
                    };
                    PreProcLineKind::Include(Include {
                        span: span.clone(),
                        kind,
                        path: i.slice(1..i.len() - 1),
                    })
                }

                _ => {
                    let span = line.start().join(&i);
                    return Err(ParseErr::new(
                        span.clone(),
                        format!("unrecognized preprocessor directive: `{span}`"),
                    ));
                }
            };

            Ok((rest, Some(Self { span, kind })))
        } else {
            Ok((input.clone(), None))
        }
    }
}