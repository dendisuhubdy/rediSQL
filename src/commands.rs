use std::ffi::{CStr, CString};
use std::os::raw::c_long;
use std::sync::mpsc::channel;
use std::thread;

use redisql_lib::redis::{
    get_ch_from_dbkeyptr, get_dbkey_from_name,
    get_dbkeyptr_from_name, reply_with_error_from_key_type,
    RedisReply,
};
use redisql_lib::redis_type::ReplicateVerbatim;
use redisql_lib::sqlite::{get_arc_connection, QueryResult};
use redisql_lib::virtual_tables as vtab;

use redisql_lib::redis as r;

use sync_engine::Replicate;

use redisql_lib::statistics::STATISTICS;

const REDISQL_VERSION: Option<&'static str> =
    option_env!("CARGO_PKG_VERSION");

extern "C" fn reply(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    _argv: *mut *mut r::rm::ffi::RedisModuleString,
    _argc: ::std::os::raw::c_int,
) -> i32 {
    let context = r::rm::Context::new(ctx);
    let result = unsafe {
        r::rm::ffi::RedisModule_GetBlockedClientPrivateData.unwrap()(
            context.as_ptr(),
        ) as *mut *mut RedisReply
    };
    let result_wrap: Box<*mut r::RedisReply> =
        unsafe { Box::from_raw(result) };
    let mut result: Box<r::RedisReply> =
        unsafe { Box::from_raw(*result_wrap) };
    result.reply(&context)
}

extern "C" fn timeout(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    _argv: *mut *mut r::rm::ffi::RedisModuleString,
    _argc: ::std::os::raw::c_int,
) -> i32 {
    unsafe { r::rm::ffi::RedisModule_ReplyWithNull.unwrap()(ctx) }
}

extern "C" fn free_privdata(_arg: *mut ::std::os::raw::c_void) {}

#[allow(non_snake_case)]
pub extern "C" fn ExecStatement(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    argv: *mut *mut r::rm::ffi::RedisModuleString,
    argc: ::std::os::raw::c_int,
) -> i32 {
    STATISTICS.exec_statement();

    let context = r::rm::Context::new(ctx);
    let argvector = match r::create_argument(argv, argc) {
        Ok(argvector) => argvector,
        Err(mut error) => {
            STATISTICS.exec_statement_err();
            return error.reply(&context);
        }
    };

    match argvector.len() {
        0...2 => {
            let error = CString::new(
                "Wrong number of arguments, it \
                 needs at least 3",
            )
            .unwrap();
            STATISTICS.exec_statement_err();
            unsafe {
                r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                    context.as_ptr(),
                    error.as_ptr(),
                )
            }
        }
        _ => {
            let db = match get_dbkeyptr_from_name(
                context.as_ptr(),
                argvector[1],
            ) {
                Ok(db) => db,
                Err(e) => {
                    STATISTICS.exec_err();
                    return reply_with_error_from_key_type(
                        context.as_ptr(),
                        e,
                    );
                }
            };

            let ch = unsafe { get_ch_from_dbkeyptr(db) };

            let blocked_client = r::rm::BlockedClient {
                client: unsafe {
                    r::rm::ffi::RedisModule_BlockClient.unwrap()(
                        context.as_ptr(),
                        Some(reply),
                        Some(timeout),
                        Some(free_privdata),
                        10000,
                    )
                },
            };
            let t = std::time::Instant::now()
                + std::time::Duration::from_secs(10);

            let cmd = r::Command::ExecStatement {
                identifier: argvector[2],
                arguments: argvector[3..].to_vec(),
                client: blocked_client,
                timeout: t,
            };

            match ch.send(cmd) {
                Ok(()) => {
                    unsafe {
                        Replicate(
                            &context,
                            "REDISQL.EXEC_STATEMENT.NOW",
                            argv,
                            argc,
                        );
                    }
                    r::rm::ffi::REDISMODULE_OK
                }
                Err(_) => r::rm::ffi::REDISMODULE_OK,
            }
        }
    }
}

#[allow(non_snake_case)]
pub extern "C" fn QueryStatement(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    argv: *mut *mut r::rm::ffi::RedisModuleString,
    argc: ::std::os::raw::c_int,
) -> i32 {
    STATISTICS.query_statement();
    let context = r::rm::Context::new(ctx);
    let argvector = match r::create_argument(argv, argc) {
        Ok(argvector) => argvector,
        Err(mut error) => {
            STATISTICS.query_statement_err();
            return error.reply(&context);
        }
    };

    match argvector.len() {
        0...2 => {
            let error = CString::new(
                "Wrong number of arguments, it \
                 needs at least 3",
            )
            .unwrap();
            STATISTICS.query_statement_err();
            unsafe {
                r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                    context.as_ptr(),
                    error.as_ptr(),
                )
            }
        }
        _ => {
            let db = match get_dbkeyptr_from_name(
                context.as_ptr(),
                argvector[1],
            ) {
                Ok(db) => db,
                Err(e) => {
                    STATISTICS.exec_err();
                    return reply_with_error_from_key_type(
                        context.as_ptr(),
                        e,
                    );
                }
            };
            let ch = unsafe { get_ch_from_dbkeyptr(db) };

            let blocked_client = r::rm::BlockedClient {
                client: unsafe {
                    r::rm::ffi::RedisModule_BlockClient.unwrap()(
                        context.as_ptr(),
                        Some(reply),
                        Some(timeout),
                        Some(free_privdata),
                        10000,
                    )
                },
            };

            let t = std::time::Instant::now()
                + std::time::Duration::from_secs(10);

            let cmd = r::Command::QueryStatement {
                identifier: argvector[2],
                arguments: argvector[3..].to_vec(),
                return_method: r::ReturnMethod::Reply,
                client: blocked_client,
                timeout: t,
            };

            match ch.send(cmd) {
                Ok(()) => r::rm::ffi::REDISMODULE_OK,
                Err(_) => r::rm::ffi::REDISMODULE_OK,
            }
        }
    }
}

#[allow(non_snake_case)]
pub extern "C" fn QueryStatementInto(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    argv: *mut *mut r::rm::ffi::RedisModuleString,
    argc: ::std::os::raw::c_int,
) -> i32 {
    STATISTICS.query_statement_into();
    let context = r::rm::Context::new(ctx);
    let argvector = match r::create_argument(argv, argc) {
        Ok(argvector) => argvector,
        Err(mut error) => {
            STATISTICS.query_statement_into_err();
            return error.reply(&context);
        }
    };

    match argvector.len() {
        0...3 => {
            let error = CString::new(
                "Wrong number of arguments, it \
                 needs at least 4",
            )
            .unwrap();
            STATISTICS.query_statement_into_err();
            unsafe {
                r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                    context.as_ptr(),
                    error.as_ptr(),
                )
            }
        }
        _ => {
            let stream_name = argvector[1];

            let db = match get_dbkeyptr_from_name(
                context.as_ptr(),
                argvector[2],
            ) {
                Ok(db) => db,
                Err(e) => {
                    STATISTICS.exec_err();
                    return reply_with_error_from_key_type(
                        context.as_ptr(),
                        e,
                    );
                }
            };
            let ch = unsafe { get_ch_from_dbkeyptr(db) };

            let blocked_client = r::rm::BlockedClient {
                client: unsafe {
                    r::rm::ffi::RedisModule_BlockClient.unwrap()(
                        context.as_ptr(),
                        Some(reply),
                        Some(timeout),
                        Some(free_privdata),
                        10000,
                    )
                },
            };

            let t = std::time::Instant::now()
                + std::time::Duration::from_secs(10);

            let cmd = r::Command::QueryStatement {
                identifier: argvector[3],
                arguments: argvector[4..].to_vec(),
                return_method: r::ReturnMethod::Stream {
                    name: stream_name,
                },
                client: blocked_client,
                timeout: t,
            };

            match ch.send(cmd) {
                Ok(()) => r::rm::ffi::REDISMODULE_OK,
                Err(_) => r::rm::ffi::REDISMODULE_OK,
            }
        }
    }
}

#[allow(non_snake_case)]
pub extern "C" fn Exec(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    argv: *mut *mut r::rm::ffi::RedisModuleString,
    argc: ::std::os::raw::c_int,
) -> i32 {
    STATISTICS.exec();
    let context = r::rm::Context::new(ctx);
    let argvector = match r::create_argument(argv, argc) {
        Ok(argvector) => argvector,
        Err(mut error) => {
            STATISTICS.exec_err();
            return error.reply(&context);
        }
    };

    match argvector.len() {
        3 => {
            let db = match get_dbkeyptr_from_name(
                context.as_ptr(),
                argvector[1],
            ) {
                Ok(db) => db,
                Err(e) => {
                    STATISTICS.exec_err();
                    return reply_with_error_from_key_type(
                        context.as_ptr(),
                        e,
                    );
                }
            };
            let ch = unsafe { get_ch_from_dbkeyptr(db) };

            let blocked_client = r::rm::BlockedClient {
                client: unsafe {
                    r::rm::ffi::RedisModule_BlockClient.unwrap()(
                        context.as_ptr(),
                        Some(reply),
                        Some(timeout),
                        Some(free_privdata),
                        10000,
                    )
                },
            };

            let t = std::time::Instant::now()
                + std::time::Duration::from_secs(10);

            let cmd = r::Command::Exec {
                query: argvector[2],
                client: blocked_client,
                timeout: t,
            };

            match ch.send(cmd) {
                Ok(()) => {
                    unsafe {
                        Replicate(
                            &context,
                            "REDISQL.EXEC.NOW",
                            argv,
                            argc,
                        );
                    }
                    STATISTICS.exec_ok();
                    r::rm::ffi::REDISMODULE_OK
                }
                Err(_) => {
                    STATISTICS.exec_err();
                    r::rm::ffi::REDISMODULE_OK
                }
            }
        }
        n => {
            let error = CString::new(format!(
                "Wrong number of arguments, it \
                 accepts 3, you provide {}",
                n
            ))
            .unwrap();
            STATISTICS.exec_err();
            unsafe {
                r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                    context.as_ptr(),
                    error.as_ptr(),
                )
            }
        }
    }
}

#[allow(non_snake_case)]
pub extern "C" fn Query(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    argv: *mut *mut r::rm::ffi::RedisModuleString,
    argc: ::std::os::raw::c_int,
) -> i32 {
    STATISTICS.query();
    let context = r::rm::Context::new(ctx);
    let argvector = match r::create_argument(argv, argc) {
        Ok(argvector) => argvector,
        Err(mut error) => {
            STATISTICS.query_err();
            return error.reply(&context);
        }
    };

    match argvector.len() {
        3 => {
            let db = match get_dbkeyptr_from_name(
                context.as_ptr(),
                argvector[1],
            ) {
                Ok(db) => db,
                Err(e) => {
                    STATISTICS.exec_err();
                    return reply_with_error_from_key_type(
                        context.as_ptr(),
                        e,
                    );
                }
            };
            let ch = unsafe { get_ch_from_dbkeyptr(db) };

            let blocked_client = r::rm::BlockedClient {
                client: unsafe {
                    r::rm::ffi::RedisModule_BlockClient.unwrap()(
                        context.as_ptr(),
                        Some(reply),
                        Some(timeout),
                        Some(free_privdata),
                        10000,
                    )
                },
            };

            let t = std::time::Instant::now()
                + std::time::Duration::from_secs(10);

            let cmd = r::Command::Query {
                query: argvector[2],
                return_method: r::ReturnMethod::Reply,
                client: blocked_client,
                timeout: t,
            };
            match ch.send(cmd) {
                Ok(()) => r::rm::ffi::REDISMODULE_OK,
                Err(_) => r::rm::ffi::REDISMODULE_OK,
            }
        }
        n => {
            let error = CString::new(format!(
                "Wrong number of arguments, it \
                 accepts 3, you provide {}",
                n
            ))
            .unwrap();
            STATISTICS.query_err();
            unsafe {
                r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                    context.as_ptr(),
                    error.as_ptr(),
                )
            }
        }
    }
}

#[allow(non_snake_case)]
pub extern "C" fn QueryInto(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    argv: *mut *mut r::rm::ffi::RedisModuleString,
    argc: ::std::os::raw::c_int,
) -> i32 {
    STATISTICS.query_into();
    let context = r::rm::Context::new(ctx);
    let argvector = match r::create_argument(argv, argc) {
        Ok(argvector) => argvector,
        Err(mut error) => {
            STATISTICS.query_into_err();
            return error.reply(&context);
        }
    };

    match argvector.len() {
        4 => {
            let stream_name = argvector[1];

            let db = match get_dbkeyptr_from_name(
                context.as_ptr(),
                argvector[2],
            ) {
                Ok(db) => db,
                Err(e) => {
                    STATISTICS.exec_err();
                    return reply_with_error_from_key_type(
                        context.as_ptr(),
                        e,
                    );
                }
            };
            let ch = unsafe { get_ch_from_dbkeyptr(db) };

            let blocked_client = r::rm::BlockedClient {
                client: unsafe {
                    r::rm::ffi::RedisModule_BlockClient.unwrap()(
                        context.as_ptr(),
                        Some(reply),
                        Some(timeout),
                        Some(free_privdata),
                        10000,
                    )
                },
            };

            let t = std::time::Instant::now()
                + std::time::Duration::from_secs(10);

            let cmd = r::Command::Query {
                query: argvector[3],
                return_method: r::ReturnMethod::Stream {
                    name: stream_name,
                },
                client: blocked_client,
                timeout: t,
            };
            match ch.send(cmd) {
                Ok(()) => r::rm::ffi::REDISMODULE_OK,
                Err(_) => r::rm::ffi::REDISMODULE_OK,
            }
        }
        n => {
            let error = CString::new(format!(
                "Wrong number of arguments, it \
                 accepts 4, you provide {}",
                n
            ))
            .unwrap();
            STATISTICS.query_into_err();
            unsafe {
                r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                    context.as_ptr(),
                    error.as_ptr(),
                )
            }
        }
    }
}

#[allow(non_snake_case)]
pub extern "C" fn CreateStatement(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    argv: *mut *mut r::rm::ffi::RedisModuleString,
    argc: ::std::os::raw::c_int,
) -> i32 {
    STATISTICS.create_statement();

    let context = r::rm::Context::new(ctx);
    let argvector = match r::create_argument(argv, argc) {
        Ok(argvector) => argvector,
        Err(mut error) => {
            STATISTICS.create_statement_err();
            return error.reply(&context);
        }
    };

    match argvector.len() {
        4 => {
            let db = match get_dbkeyptr_from_name(
                context.as_ptr(),
                argvector[1],
            ) {
                Ok(db) => db,
                Err(e) => {
                    STATISTICS.exec_err();
                    return reply_with_error_from_key_type(
                        context.as_ptr(),
                        e,
                    );
                }
            };
            let ch = unsafe { get_ch_from_dbkeyptr(db) };

            let blocked_client = r::rm::BlockedClient {
                client: unsafe {
                    r::rm::ffi::RedisModule_BlockClient.unwrap()(
                        context.as_ptr(),
                        Some(reply),
                        Some(timeout),
                        Some(free_privdata),
                        10000,
                    )
                },
            };

            let cmd = r::Command::CompileStatement {
                identifier: argvector[2],
                statement: argvector[3],
                client: blocked_client,
            };

            match ch.send(cmd) {
                Ok(()) => {
                    unsafe {
                        Replicate(
                            &context,
                            "REDISQL.CREATE_STATEMENT.NOW",
                            argv,
                            argc,
                        );
                    }
                    r::rm::ffi::REDISMODULE_OK
                }
                Err(_) => r::rm::ffi::REDISMODULE_OK,
            }
        }

        _ => {
            let error = CString::new(
                "Wrong number of arguments, it \
                 accepts 4",
            )
            .unwrap();
            STATISTICS.create_statement_err();
            unsafe {
                r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                    context.as_ptr(),
                    error.as_ptr(),
                )
            }
        }
    }
}

#[allow(non_snake_case)]
pub extern "C" fn UpdateStatement(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    argv: *mut *mut r::rm::ffi::RedisModuleString,
    argc: ::std::os::raw::c_int,
) -> i32 {
    STATISTICS.update_statement();
    let context = r::rm::Context::new(ctx);
    let argvector = match r::create_argument(argv, argc) {
        Ok(argvector) => argvector,
        Err(mut error) => {
            STATISTICS.update_statement_err();
            return error.reply(&context);
        }
    };

    match argvector.len() {
        4 => {
            let db = match get_dbkeyptr_from_name(
                context.as_ptr(),
                argvector[1],
            ) {
                Ok(db) => db,
                Err(e) => {
                    STATISTICS.exec_err();
                    return reply_with_error_from_key_type(
                        context.as_ptr(),
                        e,
                    );
                }
            };
            let ch = unsafe { get_ch_from_dbkeyptr(db) };

            let blocked_client = r::rm::BlockedClient {
                client: unsafe {
                    r::rm::ffi::RedisModule_BlockClient.unwrap()(
                        context.as_ptr(),
                        Some(reply),
                        Some(timeout),
                        Some(free_privdata),
                        10000,
                    )
                },
            };

            let cmd = r::Command::UpdateStatement {
                identifier: argvector[2],
                statement: argvector[3],
                client: blocked_client,
            };

            match ch.send(cmd) {
                Ok(()) => {
                    unsafe {
                        Replicate(
                            &context,
                            "REDISQL.UPDATE_STATEMENT.NOW",
                            argv,
                            argc,
                        );
                    }
                    r::rm::ffi::REDISMODULE_OK
                }
                Err(_) => r::rm::ffi::REDISMODULE_OK,
            }
        }

        _ => {
            let error = CString::new(
                "Wrong number of arguments, it \
                 accepts 4",
            )
            .unwrap();
            STATISTICS.update_statement_err();
            unsafe {
                r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                    context.as_ptr(),
                    error.as_ptr(),
                )
            }
        }
    }
}

#[allow(non_snake_case)]
pub extern "C" fn DeleteStatement(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    argv: *mut *mut r::rm::ffi::RedisModuleString,
    argc: ::std::os::raw::c_int,
) -> i32 {
    STATISTICS.delete_statement();

    let context = r::rm::Context::new(ctx);
    let argvector = match r::create_argument(argv, argc) {
        Ok(argvector) => argvector,
        Err(mut error) => {
            STATISTICS.delete_statement_err();
            return error.reply(&context);
        }
    };

    match argvector.len() {
        3 => {
            let db = match get_dbkeyptr_from_name(
                context.as_ptr(),
                argvector[1],
            ) {
                Ok(db) => db,
                Err(e) => {
                    STATISTICS.exec_err();
                    return reply_with_error_from_key_type(
                        context.as_ptr(),
                        e,
                    );
                }
            };
            let ch = unsafe { get_ch_from_dbkeyptr(db) };

            let blocked_client = r::rm::BlockedClient {
                client: unsafe {
                    r::rm::ffi::RedisModule_BlockClient.unwrap()(
                        context.as_ptr(),
                        Some(reply),
                        Some(timeout),
                        Some(free_privdata),
                        10000,
                    )
                },
            };

            let cmd = r::Command::DeleteStatement {
                identifier: argvector[2],
                client: blocked_client,
            };
            match ch.send(cmd) {
                Ok(()) => {
                    unsafe {
                        Replicate(
                            &context,
                            "REDISQL.DELETE_STATEMENT.NOW",
                            argv,
                            argc,
                        );
                    }
                    r::rm::ffi::REDISMODULE_OK
                }
                Err(_) => r::rm::ffi::REDISMODULE_OK,
            }
        }
        _ => {
            let error = CString::new(
                "Wrong number of arguments, it \
                 accepts 3",
            )
            .unwrap();
            STATISTICS.delete_statement_err();
            unsafe {
                r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                    context.as_ptr(),
                    error.as_ptr(),
                )
            }
        }
    }
}

#[allow(non_snake_case)]
pub extern "C" fn CreateDB(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    argv: *mut *mut r::rm::ffi::RedisModuleString,
    argc: ::std::os::raw::c_int,
) -> i32 {
    STATISTICS.create_db();
    let context = r::rm::Context::new(ctx);
    let argvector = match r::create_argument(argv, argc) {
        Ok(argvector) => argvector,
        Err(mut error) => {
            STATISTICS.create_db_err();
            return error.reply(&context);
        }
    };

    match argvector.len() {
        2 | 3 => {
            let key_name =
                r::rm::RMString::new(&context, argvector[1]);
            let key = unsafe {
                r::rm::ffi::Export_RedisModule_OpenKey(
                    context.as_ptr(),
                    key_name.as_ptr(),
                    r::rm::ffi::REDISMODULE_WRITE,
                )
            };
            let safe_key = r::RedisKey { key };
            match unsafe {
                r::rm::ffi::RedisModule_KeyType.unwrap()(safe_key.key)
            } {
                r::rm::ffi::REDISMODULE_KEYTYPE_EMPTY => {
                    let (path, in_memory): (&str, bool) =
                        match argvector.len() {
                            3 => (argvector[2], false),
                            _ => (":memory:", true),
                        };
                    match get_arc_connection(path) {
                        Ok(rc) => {
                            match r::create_metadata_table(rc.clone())
                                .and_then(|_| {
                                    r::enable_foreign_key(rc.clone())
                                })
                                .and_then(|_| {
                                    r::insert_path_metadata(
                                        rc.clone(),
                                        path,
                                    )
                                })
                                .and_then(|_| {
                                    vtab::register_modules(&rc)
                                }) {
                                Err(mut e) => e.reply(&context),
                                Ok(vtab_context) => {
                                    let (tx, rx) = channel();
                                    let db = r::DBKey::new_from_arc(
                                        tx,
                                        rc,
                                        in_memory,
                                        vtab_context,
                                    );
                                    let mut loop_data =
                                        db.loop_data.clone();
                                    thread::spawn(move || {
                                        r::listen_and_execute(
                                            &mut loop_data,
                                            &rx,
                                        );
                                    });
                                    let ptr =
                                        Box::into_raw(Box::new(db));
                                    let type_set = unsafe {
                                        r::rm::ffi::RedisModule_ModuleTypeSetValue.unwrap()(
                                        safe_key.key,
                                        r::rm::ffi::DBType,
                                        ptr as *mut std::os::raw::c_void,
                                    )
                                    };

                                    match type_set {
                                    r::rm::ffi::REDISMODULE_OK => {
                                        let mut ok =
                                            QueryResult::OK {};
                                        STATISTICS.create_db_ok();
                                        ReplicateVerbatim(&context);
                                        ok.reply(&context)
                                    }
                                    r::rm::ffi::REDISMODULE_ERR => {
                                        let err = CString::new(
                                            "ERR - Error in saving the database inside Redis",
                                        ).unwrap();
                                        STATISTICS.create_db_err();
                                        unsafe {
                                            r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                                                context.as_ptr(),
                                                err.as_ptr(),
                                            )
                                        }
                                    }
                                    _ => {
                                        let err = CString::new("ERR - Error unknow").unwrap();
                                        STATISTICS.create_db_err();
                                        unsafe {
                                            r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                                                context.as_ptr(),
                                                err.as_ptr(),
                                            )
                                        }
                                    }
                                }
                                }
                            }
                        }
                        Err(_) => {
                            let error = CString::new(
                                "Err - Error \
                                 opening the in \
                                 memory databade",
                            )
                            .unwrap();
                            STATISTICS.create_db_err();
                            unsafe {
                                r::rm::ffi::RedisModule_ReplyWithError
                                    .unwrap()(
                                    context.as_ptr(),
                                    error.as_ptr(),
                                )
                            }
                        }
                    }
                }
                _ => {
                    let error = CStr::from_bytes_with_nul(
                        r::rm::ffi::REDISMODULE_ERRORMSG_WRONGTYPE,
                    )
                    .unwrap();
                    STATISTICS.create_db_err();
                    unsafe {
                        r::rm::ffi::RedisModule_ReplyWithError
                            .unwrap()(
                            context.as_ptr(),
                            error.as_ptr(),
                        )
                    }
                }
            }
        }
        _ => {
            let error = CString::new(
                "Wrong number of arguments, it \
                 accepts 2 or 3",
            )
            .unwrap();
            STATISTICS.create_db_err();
            unsafe {
                r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                    context.as_ptr(),
                    error.as_ptr(),
                )
            }
        }
    }
}

#[allow(non_snake_case)]
pub extern "C" fn MakeCopy(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    argv: *mut *mut r::rm::ffi::RedisModuleString,
    argc: ::std::os::raw::c_int,
) -> i32 {
    debug!("MakeCopy | Start");
    STATISTICS.copy();
    let context = r::rm::Context::new(ctx);
    let argvector = match r::create_argument(argv, argc) {
        Ok(argvector) => argvector,
        Err(mut error) => {
            STATISTICS.copy_err();
            return error.reply(&context);
        }
    };

    match argvector.len() {
        3 => {
            let db = match get_dbkeyptr_from_name(
                context.as_ptr(),
                argvector[1],
            ) {
                Ok(db) => db,
                Err(e) => {
                    STATISTICS.exec_err();
                    return reply_with_error_from_key_type(
                        context.as_ptr(),
                        e,
                    );
                }
            };

            let ch = unsafe { get_ch_from_dbkeyptr(db) };
            let dest_db =
                get_dbkey_from_name(context.as_ptr(), argvector[2]);
            if dest_db.is_err() {
                let error = CString::new(
                    "Error in opening the DESTINATION database",
                )
                .unwrap();
                STATISTICS.copy_err();
                return unsafe {
                    r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                        context.as_ptr(),
                        error.as_ptr(),
                    )
                };
            }
            let dest_db = dest_db.unwrap();

            let blocked_client = r::rm::BlockedClient {
                client: unsafe {
                    r::rm::ffi::RedisModule_BlockClient.unwrap()(
                        context.as_ptr(),
                        Some(reply),
                        Some(timeout),
                        Some(free_privdata),
                        10000,
                    )
                },
            };
            let cmd = r::Command::MakeCopy {
                destination: dest_db,
                client: blocked_client,
            };

            match ch.send(cmd) {
                Ok(()) => {
                    debug!("MakeCopy | Successfully send command");
                    unsafe {
                        Replicate(
                            &context,
                            "REDISQL.COPY.NOW",
                            argv,
                            argc,
                        );
                    }
                    r::rm::ffi::REDISMODULE_OK
                }
                Err(_) => r::rm::ffi::REDISMODULE_OK,
            }
        }
        _ => {
            let error = CString::new(
                "Wrong number of arguments, it accepts exactly 3",
            )
            .unwrap();
            STATISTICS.copy_err();
            unsafe {
                r::rm::ffi::RedisModule_ReplyWithError.unwrap()(
                    context.as_ptr(),
                    error.as_ptr(),
                )
            }
        }
    }
}

#[allow(non_snake_case)]
pub extern "C" fn GetStatistics(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    _argv: *mut *mut r::rm::ffi::RedisModuleString,
    _argc: ::std::os::raw::c_int,
) -> i32 {
    let context = r::rm::Context::new(ctx);
    let data = STATISTICS.values().data;

    let len = data.len() as c_long;
    unsafe {
        r::rm::ffi::RedisModule_ReplyWithArray.unwrap()(
            context.as_ptr(),
            len,
        );
    }
    for statics in data {
        unsafe {
            r::rm::ffi::RedisModule_ReplyWithArray.unwrap()(
                context.as_ptr(),
                2,
            );
        }
        r::rm::ReplyWithStringBuffer(&context, statics.0.as_bytes());
        r::rm::ReplyWithLongLong(&context, statics.1 as i64);
    }

    r::rm::ffi::REDISMODULE_OK
}

#[allow(non_snake_case)]
pub extern "C" fn RediSQLVersion(
    ctx: *mut r::rm::ffi::RedisModuleCtx,
    _argv: *mut *mut r::rm::ffi::RedisModuleString,
    _argc: ::std::os::raw::c_int,
) -> i32 {
    let context = r::rm::Context::new(ctx);
    let version = REDISQL_VERSION.unwrap_or("unknown");
    r::rm::ReplyWithStringBuffer(&context, version.as_bytes());

    r::rm::ffi::REDISMODULE_OK
}
