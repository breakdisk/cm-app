package io.logisticos.driver.core.database

import androidx.room.Database
import androidx.room.RoomDatabase
import androidx.room.TypeConverters
import io.logisticos.driver.core.database.dao.*
import io.logisticos.driver.core.database.entity.*

@TypeConverters(Converters::class)
@Database(
    entities = [
        ShiftEntity::class,
        TaskEntity::class,
        RouteEntity::class,
        PodEntity::class,
        LocationBreadcrumbEntity::class,
        ScanEventEntity::class,
        SyncQueueEntity::class,
    ],
    version = 3,
    exportSchema = true
)
abstract class DriverDatabase : RoomDatabase() {
    abstract fun shiftDao(): ShiftDao
    abstract fun taskDao(): TaskDao
    abstract fun routeDao(): RouteDao
    abstract fun podDao(): PodDao
    abstract fun locationBreadcrumbDao(): LocationBreadcrumbDao
    abstract fun scanEventDao(): ScanEventDao
    abstract fun syncQueueDao(): SyncQueueDao
}
