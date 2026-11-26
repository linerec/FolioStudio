use folio_edit::{Command,commands::RegisteredCommand};
pub fn prepare(commands:&mut [Command],session:&crate::Session)->Result<(),String>{
    let convert=|c:&Command|matches!(c,Command::Registered(RegisteredCommand::ConvertListsToText{..}|RegisteredCommand::ConvertStyleListsToText{..}));
    if !commands.iter().any(convert){return Ok(())}
    if commands.len()!=1 {
        let mut stories=std::collections::BTreeSet::new();
        for command in commands.iter() {
            match command {
                Command::Registered(RegisteredCommand::ConvertListsToText{story,..}) if stories.insert(story.clone()) => {},
                _ => return Err("convert distinct stories or cells in a separate edit".into()),
            }
        }
    }
    for command in commands {
        if let Command::Registered(RegisteredCommand::ConvertListsToText{story,labels,..})=command {
            *labels=folio_text::layout::listconvert::labels(&session.doc,story)?;
        }
        if let Command::Registered(RegisteredCommand::ConvertStyleListsToText{labels,..})=command {
            *labels=folio_text::layout::listconvert::document_labels(&session.doc)?;
        }
    }
    Ok(())
}
